use crate::net::{self, OutgoingFileMeta};
use crate::state::{guess_mime, now_ms, AppState};
use crate::windows;
use base64::Engine;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use uuid::Uuid;

async fn socket_of(state: &AppState) -> Option<Arc<tokio::net::UdpSocket>> {
    state.udp_socket.lock().await.clone()
}

#[tauri::command]
pub fn get_init(state: State<'_, Arc<AppState>>) -> Value {
    let username = state.my_username.lock().unwrap().clone();
    let avatar = state.settings.lock().unwrap().my_avatar.clone();
    json!({
        "myId": state.my_id,
        "username": username,
        "peers": state.peer_list_for_client(),
        "avatar": avatar,
    })
}

#[tauri::command]
pub async fn set_username(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    username: String,
) -> Result<Value, ()> {
    let state = state.inner().clone();
    let is_first_time = state.my_username.lock().unwrap().is_none();
    let clean: String = username.trim().chars().take(40).collect();
    let clean = if clean.is_empty() { "Anonimo".to_string() } else { clean };
    *state.my_username.lock().unwrap() = Some(clean.clone());

    if let Some(socket) = socket_of(&state).await {
        net::broadcast_presence_now(&state, &socket).await;
    }

    if is_first_time {
        state.windows_created.store(true, Ordering::SeqCst);
        let _ = windows::create_panel_window(&app);
        let _ = windows::create_chat_window(&app);
        let _ = windows::create_quickreply_window(&app);
        let _ = windows::create_tray(&app, state.clone());

        if let Some(login) = app.get_webview_window("login") {
            let _ = login.close();
        }
        windows::show_panel_window(&app, &state);
    }

    Ok(json!({ "myId": state.my_id, "username": clean }))
}

#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    to_id: String,
    text: String,
) -> Result<Value, ()> {
    let state = state.inner().clone();
    let socket = socket_of(&state).await;
    let ok = match &socket {
        Some(s) => net::send_message_to_peer(&state, s, &to_id, &text).await,
        None => false,
    };
    let timestamp = now_ms();
    if ok {
        let username = state.peers.lock().unwrap().get(&to_id).map(|p| p.username.clone());
        let msg = json!({ "fromMe": true, "text": text, "timestamp": timestamp, "seen": false });
        state.append_to_conversation(&to_id, username.as_deref().unwrap_or(""), msg.clone());

        // Igual que en la versión Electron: el mensaje propio se agrega a la
        // ventana de chat vía este evento (si está abierta y mirando a esta
        // persona), no lo agrega "a mano" el formulario del lado de React.
        let active = state.active_peer_id.lock().unwrap().clone();
        if active.as_deref() == Some(to_id.as_str()) {
            let _ = app.emit_to("chat", "message-received", msg.clone());
        }

        mark_conversation_as_read(&app, &state, &to_id, timestamp).await;
    }
    Ok(json!({ "ok": ok, "timestamp": timestamp }))
}

async fn mark_conversation_as_read(app: &AppHandle, state: &Arc<AppState>, peer_id: &str, upto: i64) {
    let had_unread = state.unread_counts.lock().unwrap().remove(peer_id).is_some();
    if had_unread {
        windows::broadcast_peers_updated(app, state);
    }
    if let Some(socket) = socket_of(state).await {
        net::send_seen_to_peer(state, &socket, peer_id, upto).await;
    }
}

#[tauri::command]
pub async fn open_chat(app: AppHandle, state: State<'_, Arc<AppState>>, peer_id: String) -> Result<bool, ()> {
    windows::open_chat_with(app, state.inner().clone(), peer_id);
    Ok(true)
}

#[tauri::command]
pub fn back_to_panel(app: AppHandle, state: State<'_, Arc<AppState>>) -> bool {
    windows::show_panel_window(&app, state.inner());
    true
}

#[tauri::command]
pub fn hide_window(window: tauri::WebviewWindow, state: State<'_, Arc<AppState>>) -> bool {
    let _ = window.hide();
    if window.label() == "chat" {
        *state.active_peer_id.lock().unwrap() = None;
    }
    true
}

#[tauri::command]
pub fn get_autostart(state: State<'_, Arc<AppState>>) -> bool {
    state.settings.lock().unwrap().auto_launch
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, state: State<'_, Arc<AppState>>, enabled: bool) -> bool {
    apply_autostart_setting(&app, state.inner(), enabled);
    state.settings.lock().unwrap().auto_launch
}

/// Aplica y persiste la preferencia de inicio automático con Windows/macOS/Linux.
pub fn apply_autostart_setting(app: &AppHandle, state: &Arc<AppState>, enabled: bool) {
    state.settings.lock().unwrap().auto_launch = enabled;
    state.save_settings();
    let manager = app.autolaunch();
    let result = if enabled { manager.enable() } else { manager.disable() };
    if let Err(e) = result {
        eprintln!("No se pudo aplicar el inicio automático: {e}");
    }
}

async fn send_outgoing_file(app: &AppHandle, state: &Arc<AppState>, peer_id: &str, file_path: String) -> Value {
    let peer_ip = state.peers.lock().unwrap().get(peer_id).map(|p| p.ip.clone());
    let peer_ip = match peer_ip {
        Some(ip) => ip,
        None => return json!({ "error": true }),
    };

    let metadata = match tokio::fs::metadata(&file_path).await {
        Ok(m) => m,
        Err(_) => return json!({ "error": true }),
    };
    let file_name = std::path::Path::new(&file_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "archivo".to_string());
    let mime_type = guess_mime(&file_name);
    let timestamp = now_ms();
    let file_size = metadata.len();
    let temp_id = Uuid::new_v4().to_string();

    let _ = app.emit_to(
        "chat",
        "file-send-start",
        json!({
            "tempId": temp_id, "peerId": peer_id, "fileName": file_name,
            "fileSize": file_size, "mimeType": mime_type, "timestamp": timestamp
        }),
    );

    let meta = OutgoingFileMeta {
        file_path: file_path.clone(),
        file_name: file_name.clone(),
        file_size,
        mime_type: mime_type.clone(),
        timestamp,
    };
    let ok = net::send_file_to_peer(app, state, &peer_ip, &meta, &temp_id).await;

    if !ok {
        let _ = app.emit_to("chat", "file-send-error", json!({ "tempId": temp_id }));
        return json!({ "error": true, "tempId": temp_id });
    }

    let username = state.peers.lock().unwrap().get(peer_id).map(|p| p.username.clone());
    let msg = json!({
        "fromMe": true, "type": "file", "fileName": file_name, "filePath": file_path,
        "fileSize": file_size, "mimeType": mime_type, "timestamp": timestamp,
        "seen": false, "tempId": temp_id,
    });
    state.append_to_conversation(peer_id, username.as_deref().unwrap_or(""), msg.clone());
    let had_unread = state.unread_counts.lock().unwrap().remove(peer_id).is_some();
    if had_unread {
        windows::broadcast_peers_updated(app, state);
    }
    if let Some(socket) = socket_of(state).await {
        net::send_seen_to_peer(state, &socket, peer_id, timestamp).await;
    }
    msg
}

#[tauri::command]
pub async fn send_file(app: AppHandle, state: State<'_, Arc<AppState>>, peer_id: String) -> Result<Option<Value>, ()> {
    let state = state.inner().clone();
    if state.peers.lock().unwrap().get(&peer_id).is_none() {
        return Ok(None);
    }
    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app2.dialog()
            .file()
            .add_filter("Imágenes", &["jpg", "jpeg", "png", "gif", "webp", "bmp"])
            .add_filter("Todos los archivos", &["*"])
            .set_title("Enviar archivo o foto")
            .blocking_pick_file()
    })
    .await
    .unwrap_or(None);

    let path = match picked.and_then(|p| p.into_path().ok()) {
        Some(p) => p.to_string_lossy().to_string(),
        None => return Ok(None),
    };
    Ok(Some(send_outgoing_file(&app, &state, &peer_id, path).await))
}

#[tauri::command]
pub async fn send_file_path(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    file_path: String,
) -> Result<Value, ()> {
    if file_path.is_empty() {
        return Ok(json!({ "error": true }));
    }
    Ok(send_outgoing_file(&app, state.inner(), &peer_id, file_path).await)
}

#[tauri::command]
pub async fn send_clipboard_image(app: AppHandle, state: State<'_, Arc<AppState>>, peer_id: String) -> Result<Value, ()> {
    let state = state.inner().clone();
    if state.peers.lock().unwrap().get(&peer_id).is_none() {
        return Ok(json!({ "error": true }));
    }

    let img = match app.clipboard().read_image() {
        Ok(img) => img,
        Err(_) => return Ok(json!({ "error": true, "empty": true })),
    };
    let (width, height) = (img.width(), img.height());
    let rgba = img.rgba().to_vec();
    if width == 0 || height == 0 {
        return Ok(json!({ "error": true, "empty": true }));
    }

    let file_name = format!("Pegado-{}.png", now_ms());
    let dest_path = crate::state::unique_dest_path(&state.sent_dir, &file_name);
    let save_result = tokio::task::spawn_blocking({
        let dest_path = dest_path.clone();
        move || -> Result<(), String> {
            let buffer = image::RgbaImage::from_raw(width, height, rgba)
                .ok_or_else(|| "buffer inválido".to_string())?;
            buffer.save(&dest_path).map_err(|e| e.to_string())
        }
    })
    .await;

    if !matches!(save_result, Ok(Ok(()))) {
        return Ok(json!({ "error": true }));
    }

    Ok(send_outgoing_file(&app, &state, &peer_id, dest_path.to_string_lossy().to_string()).await)
}

#[tauri::command]
pub async fn pick_avatar(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<Option<Value>, ()> {
    let state = state.inner().clone();
    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app2.dialog()
            .file()
            .add_filter("Imágenes", &["jpg", "jpeg", "png", "gif", "webp", "bmp"])
            .set_title("Elegí una foto de perfil")
            .blocking_pick_file()
    })
    .await
    .unwrap_or(None);

    let path = match picked.and_then(|p| p.into_path().ok()) {
        Some(p) => p.to_string_lossy().to_string(),
        None => return Ok(None),
    };

    let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let img = image::open(&path).map_err(|e| e.to_string())?;
        let (w, h) = (img.width(), img.height());
        let side = w.min(h);
        let x = (w - side) / 2;
        let y = (h - side) / 2;
        let cropped = img.crop_imm(x, y, side, side);
        let resized = cropped.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
        let rgb = resized.to_rgb8();
        let mut bytes: Vec<u8> = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 60);
        encoder
            .encode(rgb.as_raw(), 64, 64, image::ExtendedColorType::Rgb8)
            .map_err(|e| e.to_string())?;
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        Ok(format!("data:image/jpeg;base64,{}", b64))
    })
    .await;

    let data_url = match result {
        Ok(Ok(url)) => url,
        _ => return Ok(Some(json!({ "error": true }))),
    };

    state.settings.lock().unwrap().my_avatar = Some(data_url.clone());
    state.save_settings();

    // Lo mandamos varias veces seguidas para que a los demás les llegue
    // rápido, igual que en la versión Electron.
    if let Some(socket) = socket_of(&state).await {
        net::broadcast_presence_now(&state, &socket).await;
        let state2 = state.clone();
        let socket2 = socket.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            net::broadcast_presence_now(&state2, &socket2).await;
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            net::broadcast_presence_now(&state2, &socket2).await;
        });
    }

    Ok(Some(json!({ "avatar": data_url })))
}

#[tauri::command]
pub async fn send_typing(state: State<'_, Arc<AppState>>, peer_id: String) -> Result<bool, ()> {
    let state = state.inner().clone();
    let ok = match socket_of(&state).await {
        Some(socket) => net::send_typing_to_peer(&state, &socket, &peer_id).await,
        None => false,
    };
    Ok(ok)
}

#[tauri::command]
pub fn open_file(app: AppHandle, file_path: String) -> bool {
    if !file_path.is_empty() {
        let _ = app.opener().open_path(file_path, None::<&str>);
    }
    true
}
