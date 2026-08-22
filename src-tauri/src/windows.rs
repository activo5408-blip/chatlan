use crate::net;
use crate::state::{
    now_ms, AppState, CHAT_HEIGHT, CHAT_WIDTH, PANEL_HEIGHT, PANEL_WIDTH, QUICK_REPLY_HEIGHT,
    QUICK_REPLY_WIDTH,
};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

fn common_builder<'a>(app: &'a AppHandle, label: &str) -> WebviewWindowBuilder<'a, tauri::Wry, AppHandle> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title("ChatLAN")
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .always_on_top(true)
        .visible(false)
        .shadow(false)
}

pub fn create_login_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let win = common_builder(app, "login")
        .inner_size(320.0, 360.0)
        .center()
        .skip_taskbar(false)
        .visible(true)
        .build()?;
    Ok(win)
}

pub fn create_panel_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let win = common_builder(app, "panel")
        .inner_size(PANEL_WIDTH, PANEL_HEIGHT)
        .skip_taskbar(true)
        .build()?;
    Ok(win)
}

pub fn create_chat_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let win = common_builder(app, "chat")
        .inner_size(CHAT_WIDTH, CHAT_HEIGHT)
        .skip_taskbar(true)
        .build()?;
    Ok(win)
}

pub fn create_quickreply_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let win = common_builder(app, "quickreply")
        .inner_size(QUICK_REPLY_WIDTH, QUICK_REPLY_HEIGHT)
        .skip_taskbar(true)
        .build()?;
    Ok(win)
}

/// Ubica una ventana flotante abajo a la derecha del monitor principal,
/// igual que `positionFlyout()` en la versión Electron.
pub fn position_flyout(win: &WebviewWindow, width: f64, height: f64) {
    if let Ok(Some(monitor)) = win.primary_monitor() {
        let scale = monitor.scale_factor();
        let msize = monitor.size();
        let mpos = monitor.position();
        let margin = 12.0;
        // Trabajamos en coordenadas lógicas para no tener que lidiar con el
        // escalado de pantalla a mano.
        let mx = mpos.x as f64 / scale;
        let my = mpos.y as f64 / scale;
        let mw = msize.width as f64 / scale;
        let mh = msize.height as f64 / scale;
        let x = mx + mw - width - margin;
        let y = my + mh - height - margin;
        let _ = win.set_position(tauri::Position::Logical(LogicalPosition::new(x, y)));
    }
    let _ = win.set_size(tauri::Size::Logical(LogicalSize::new(width, height)));
}

fn send_to(app: &AppHandle, label: &str, event: &str, payload: Value) {
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.emit(event, payload);
    }
}

pub fn broadcast_peers_updated(app: &AppHandle, state: &Arc<AppState>) {
    let payload = json!(state.peer_list_for_client());
    send_to(app, "panel", "peers-updated", payload.clone());
    send_to(app, "chat", "peers-updated", payload);
}

fn window_visible(app: &AppHandle, label: &str) -> bool {
    app.get_webview_window(label)
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false)
}

pub fn handle_incoming_typing(app: &AppHandle, state: &Arc<AppState>, from_id: &str) {
    let active = state.active_peer_id.lock().unwrap().clone();
    if window_visible(app, "chat") && active.as_deref() == Some(from_id) {
        send_to(app, "chat", "typing-received", json!({ "peerId": from_id }));
    }
}

pub fn notify_seen_updated(app: &AppHandle, state: &Arc<AppState>, peer_id: &str, upto: i64) {
    let active = state.active_peer_id.lock().unwrap().clone();
    if window_visible(app, "chat") && active.as_deref() == Some(peer_id) {
        send_to(app, "chat", "seen-updated", json!({ "peerId": peer_id, "upto": upto }));
    }
}

/// Sonido de aviso para un mensaje nuevo cuando el chat con esa persona no
/// está abierto. Se dejó de usar el toast nativo (no permite responder desde
/// ahí salvo en macOS) a favor de solo un sonido + la ventanita de respuesta
/// rápida, igual que en la versión Electron.
fn play_notification_sound() {
    #[cfg(target_os = "windows")]
    {
        // Sonido de sistema estándar sin dependencias extra.
        print!("\x07");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
    #[cfg(not(target_os = "windows"))]
    {
        print!("\x07");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
}

pub fn handle_incoming_message(
    app: &AppHandle,
    state: &Arc<AppState>,
    from_id: &str,
    from_name: &str,
    msg: Value,
) {
    state.append_to_conversation(from_id, from_name, msg.clone());

    let active = state.active_peer_id.lock().unwrap().clone();
    let chat_open_with_sender = window_visible(app, "chat") && active.as_deref() == Some(from_id);

    if chat_open_with_sender {
        send_to(app, "chat", "message-received", msg.clone());
        let ts = msg.get("timestamp").and_then(|v| v.as_i64()).unwrap_or_else(now_ms);
        spawn_send_seen(app.clone(), state.clone(), from_id.to_string(), ts);
    } else {
        {
            let mut unread = state.unread_counts.lock().unwrap();
            *unread.entry(from_id.to_string()).or_insert(0) += 1;
        }
        broadcast_peers_updated(app, state);
        let body = if msg.get("type").and_then(|v| v.as_str()) == Some("file") {
            format!("\u{1F4CE} {}", msg.get("fileName").and_then(|v| v.as_str()).unwrap_or(""))
        } else {
            msg.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string()
        };
        play_notification_sound();
        show_quick_reply(app, state, from_id, &body);
    }
}

pub fn show_quick_reply(app: &AppHandle, state: &Arc<AppState>, peer_id: &str, preview: &str) {
    let peer = state.peers.lock().unwrap().get(peer_id).cloned();
    let peer = match peer {
        Some(p) => p,
        None => return,
    };
    let win = match app.get_webview_window("quickreply") {
        Some(w) => w,
        None => return,
    };
    position_flyout(&win, QUICK_REPLY_WIDTH, QUICK_REPLY_HEIGHT);
    send_to(
        app,
        "quickreply",
        "quick-reply-data",
        json!({
            "peer": { "id": peer.id, "username": peer.username, "avatar": peer.avatar },
            "preview": preview,
        }),
    );
    let _ = win.show();
    let _ = win.set_focus();
}

pub fn show_panel_window(app: &AppHandle, state: &Arc<AppState>) {
    if let Some(chat) = app.get_webview_window("chat") {
        if chat.is_visible().unwrap_or(false) {
            let _ = chat.hide();
        }
    }
    *state.active_peer_id.lock().unwrap() = None;
    if let Some(panel) = app.get_webview_window("panel") {
        position_flyout(&panel, PANEL_WIDTH, PANEL_HEIGHT);
        let _ = panel.emit("peers-updated", json!(state.peer_list_for_client()));
        let _ = panel.show();
        let _ = panel.set_focus();
    }
}

fn spawn_send_seen(app: AppHandle, state: Arc<AppState>, peer_id: String, upto: i64) {
    tauri::async_runtime::spawn(async move {
        let socket = state.udp_socket.lock().await.clone();
        if let Some(socket) = socket {
            net::send_seen_to_peer(&state, &socket, &peer_id, upto).await;
        }
        let _ = app; // reservado por si se quiere emitir algo a futuro
    });
}

pub fn open_chat_with(app: AppHandle, state: Arc<AppState>, peer_id: String) {
    let peer = state.peers.lock().unwrap().get(&peer_id).cloned();
    let peer = match peer {
        Some(p) => p,
        None => return,
    };

    if let Some(qr) = app.get_webview_window("quickreply") {
        if qr.is_visible().unwrap_or(false) {
            let _ = qr.hide();
        }
    }

    *state.active_peer_id.lock().unwrap() = Some(peer_id.clone());
    state.unread_counts.lock().unwrap().remove(&peer_id);
    broadcast_peers_updated(&app, &state);

    if let Some(panel) = app.get_webview_window("panel") {
        if panel.is_visible().unwrap_or(false) {
            let _ = panel.hide();
        }
    }

    let history = state.get_conversation(&peer_id, &peer.username);
    if let Some(chat) = app.get_webview_window("chat") {
        position_flyout(&chat, CHAT_WIDTH, CHAT_HEIGHT);
        let _ = chat.emit(
            "active-peer",
            json!({
                "peer": { "id": peer.id, "username": peer.username, "online": true, "avatar": peer.avatar },
                "history": history,
            }),
        );
        let _ = chat.show();
        let _ = chat.set_focus();
    }

    spawn_send_seen(app, state, peer_id, now_ms());
}

pub fn toggle_from_tray(app: &AppHandle, state: &Arc<AppState>) {
    let panel_visible = window_visible(app, "panel");
    let chat_visible = window_visible(app, "chat");
    if panel_visible || chat_visible {
        if let Some(w) = app.get_webview_window("panel") {
            let _ = w.hide();
        }
        if let Some(w) = app.get_webview_window("chat") {
            let _ = w.hide();
        }
        *state.active_peer_id.lock().unwrap() = None;
    } else {
        show_panel_window(app, state);
    }
}

pub fn create_tray(app: &AppHandle, state: Arc<AppState>) -> tauri::Result<()> {
    let open_item = MenuItemBuilder::with_id("open", "Abrir contactos").build(app)?;
    let sep1 = PredefinedMenuItemBuilder::separator().build(app)?;
    let checked = state.settings.lock().unwrap().auto_launch;
    let autostart_item = CheckMenuItemBuilder::with_id("autostart", "Iniciar con Windows")
        .checked(checked)
        .build(app)?;
    let sep2 = PredefinedMenuItemBuilder::separator().build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Salir").build(app)?;

    let menu = MenuBuilder::new(app)
        .items(&[&open_item, &sep1, &autostart_item, &sep2, &quit_item])
        .build()?;

    let icon = app.default_window_icon().cloned();
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("ChatLAN")
        .menu(&menu)
        .show_menu_on_left_click(false);
    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    let state_for_menu = state.clone();
    let state_for_click = state.clone();

    let tray = builder
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_panel_window(app, &state_for_menu),
            "autostart" => {
                let new_val = !state_for_menu.settings.lock().unwrap().auto_launch;
                crate::commands::apply_autostart_setting(app, &state_for_menu, new_val);
                autostart_item.set_checked(new_val).ok();
            }
            "quit" => {
                state_for_menu.is_quitting.store(true, Ordering::SeqCst);
                let app2 = app.clone();
                let st2 = state_for_menu.clone();
                tauri::async_runtime::spawn(async move {
                    let socket = st2.udp_socket.lock().await.clone();
                    if let Some(socket) = socket {
                        net::send_bye_broadcast(&st2, &socket).await;
                    }
                    app2.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_from_tray(tray.app_handle(), &state_for_click);
            }
        })
        .build(app)?;
    let _ = tray;
    Ok(())
}
