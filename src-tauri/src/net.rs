use crate::state::{
    guess_mime, now_ms, unique_dest_path, AppState, PeerInfo, CLEANUP_INTERVAL_MS, DISCOVERY_PORT,
    FILE_PORT, PEER_TIMEOUT_MS, PRESENCE_INTERVAL_MS,
};
use crate::windows;
use serde_json::{json, Value};
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

/// Junta las direcciones de broadcast (255.255.255.255 de cada red local a la
/// que está conectada esta PC), igual que hacía `getBroadcastAddresses()` en
/// la versión Electron.
pub fn get_broadcast_addresses() -> Vec<String> {
    let mut result = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let if_addrs::IfAddr::V4(v4) = iface.addr {
                if let Some(bcast) = v4.broadcast {
                    result.push(bcast.to_string());
                } else {
                    let ip = u32::from(v4.ip);
                    let mask = u32::from(v4.netmask);
                    let bcast = Ipv4Addr::from(ip | !mask);
                    result.push(bcast.to_string());
                }
            }
        }
    }
    result.sort();
    result.dedup();
    if result.is_empty() {
        result.push("255.255.255.255".to_string());
    }
    result
}

pub async fn setup_udp_socket() -> std::io::Result<Arc<UdpSocket>> {
    let socket = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).await?;
    socket.set_broadcast(true)?;
    Ok(Arc::new(socket))
}

/// Arranca el loop que recibe mensajes UDP (presencia/bye/mensaje/typing/seen)
/// y los despacha. Corre para siempre en una tarea de tokio.
pub fn start_udp_receive_loop(app: AppHandle, state: Arc<AppState>, socket: Arc<UdpSocket>) {
    tauri::async_runtime::spawn(async move {
        let mut buf = [0u8; 65536];
        loop {
            let (len, addr) = match socket.recv_from(&mut buf).await {
                Ok(v) => v,
                Err(_) => continue,
            };
            let data: Value = match serde_json::from_slice(&buf[..len]) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let my_id = state.my_id.clone();
            let sender_id = data.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if sender_id.is_empty() || sender_id == my_id {
                continue;
            }
            let msg_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let ip = addr.ip().to_string();

            match msg_type {
                "presence" => {
                    let username = data
                        .get("username")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let avatar = data
                        .get("avatar")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    {
                        let mut peers = state.peers.lock().unwrap();
                        peers.insert(
                            sender_id.to_string(),
                            PeerInfo {
                                id: sender_id.to_string(),
                                username,
                                ip,
                                last_seen: now_ms(),
                                avatar,
                            },
                        );
                    }
                    windows::broadcast_peers_updated(&app, &state);
                }
                "bye" => {
                    let removed = state.peers.lock().unwrap().remove(sender_id).is_some();
                    if removed {
                        windows::broadcast_peers_updated(&app, &state);
                    }
                }
                "message" => {
                    let from_id = data
                        .get("fromId")
                        .and_then(|v| v.as_str())
                        .unwrap_or(sender_id)
                        .to_string();
                    let from_name = data
                        .get("fromName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let text = data.get("text").and_then(|v| v.as_str()).unwrap_or("");
                    let timestamp = data.get("timestamp").and_then(|v| v.as_i64()).unwrap_or_else(now_ms);
                    let msg = json!({ "fromMe": false, "text": text, "timestamp": timestamp });
                    windows::handle_incoming_message(&app, &state, &from_id, &from_name, msg);
                }
                "typing" => {
                    let from_id = data
                        .get("fromId")
                        .and_then(|v| v.as_str())
                        .unwrap_or(sender_id);
                    windows::handle_incoming_typing(&app, &state, from_id);
                }
                "seen" => {
                    let from_id = data
                        .get("fromId")
                        .and_then(|v| v.as_str())
                        .unwrap_or(sender_id);
                    let upto = data.get("upto").and_then(|v| v.as_i64()).unwrap_or(0);
                    if state.mark_messages_seen_by(from_id, upto) {
                        windows::notify_seen_updated(&app, &state, from_id, upto);
                    }
                }
                _ => {}
            }
        }
    });
}

/// Anuncio periódico de presencia (cada PRESENCE_INTERVAL_MS mientras haya
/// un nombre de usuario elegido).
pub fn spawn_presence_task(state: Arc<AppState>, socket: Arc<UdpSocket>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(PRESENCE_INTERVAL_MS));
        loop {
            interval.tick().await;
            let has_username = state.my_username.lock().unwrap().is_some();
            if has_username {
                broadcast_presence_now(&state, &socket).await;
            }
        }
    });
}

/// Limpieza periódica de peers que dejaron de anunciarse (se fueron de la red
/// o se colgó la PC sin avisar "bye").
pub fn spawn_cleanup_task(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(CLEANUP_INTERVAL_MS));
        loop {
            interval.tick().await;
            let now = now_ms();
            let mut changed = false;
            {
                let mut peers = state.peers.lock().unwrap();
                let stale: Vec<String> = peers
                    .iter()
                    .filter(|(_, p)| now - p.last_seen > PEER_TIMEOUT_MS)
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in stale {
                    peers.remove(&id);
                    changed = true;
                }
            }
            if changed {
                windows::broadcast_peers_updated(&app, &state);
            }
        }
    });
}

pub async fn broadcast_presence_now(state: &AppState, socket: &UdpSocket) {
    let username = match state.my_username.lock().unwrap().clone() {
        Some(u) => u,
        None => return,
    };
    let avatar = state.settings.lock().unwrap().my_avatar.clone();
    let payload = json!({
        "type": "presence",
        "id": state.my_id,
        "username": username,
        "avatar": avatar,
    });
    let bytes = serde_json::to_vec(&payload).unwrap();
    for bcast in get_broadcast_addresses() {
        let _ = socket.send_to(&bytes, (bcast.as_str(), DISCOVERY_PORT)).await;
    }
}

pub async fn send_bye_broadcast(state: &AppState, socket: &UdpSocket) {
    let payload = json!({ "type": "bye", "id": state.my_id });
    let bytes = serde_json::to_vec(&payload).unwrap();
    for bcast in get_broadcast_addresses() {
        let _ = socket.send_to(&bytes, (bcast.as_str(), DISCOVERY_PORT)).await;
    }
}

fn peer_ip(state: &AppState, peer_id: &str) -> Option<String> {
    state.peers.lock().unwrap().get(peer_id).map(|p| p.ip.clone())
}

pub async fn send_message_to_peer(state: &AppState, socket: &UdpSocket, peer_id: &str, text: &str) -> bool {
    let ip = match peer_ip(state, peer_id) {
        Some(ip) => ip,
        None => return false,
    };
    let username = state.my_username.lock().unwrap().clone();
    let payload = json!({
        "type": "message",
        "id": state.my_id,
        "fromId": state.my_id,
        "fromName": username,
        "text": text,
        "timestamp": now_ms(),
    });
    let bytes = serde_json::to_vec(&payload).unwrap();
    socket.send_to(&bytes, (ip.as_str(), DISCOVERY_PORT)).await.is_ok()
}

pub async fn send_typing_to_peer(state: &AppState, socket: &UdpSocket, peer_id: &str) -> bool {
    let ip = match peer_ip(state, peer_id) {
        Some(ip) => ip,
        None => return false,
    };
    let username = state.my_username.lock().unwrap().clone();
    let payload = json!({
        "type": "typing",
        "id": state.my_id,
        "fromId": state.my_id,
        "fromName": username,
    });
    let bytes = serde_json::to_vec(&payload).unwrap();
    socket.send_to(&bytes, (ip.as_str(), DISCOVERY_PORT)).await.is_ok()
}

pub async fn send_seen_to_peer(state: &AppState, socket: &UdpSocket, peer_id: &str, upto: i64) -> bool {
    let ip = match peer_ip(state, peer_id) {
        Some(ip) => ip,
        None => return false,
    };
    let payload = json!({
        "type": "seen",
        "id": state.my_id,
        "fromId": state.my_id,
        "upto": upto,
    });
    let bytes = serde_json::to_vec(&payload).unwrap();
    socket.send_to(&bytes, (ip.as_str(), DISCOVERY_PORT)).await.is_ok()
}

// ---------------- TCP: transferencia de archivos/fotos P2P ----------------
// Framing: [4 bytes BE = largo del header JSON][header JSON][cuerpo del archivo, streaming]

pub fn start_file_server(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind(("0.0.0.0", FILE_PORT)).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("No se pudo abrir el puerto de archivos: {e}");
                return;
            }
        };
        loop {
            let (socket, _addr) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => continue,
            };
            let app = app.clone();
            let state = state.clone();
            tauri::async_runtime::spawn(async move {
                let _ = handle_incoming_file(app, state, socket).await;
            });
        }
    });
}

async fn handle_incoming_file(
    app: AppHandle,
    state: Arc<AppState>,
    mut socket: TcpStream,
) -> std::io::Result<()> {
    let mut len_buf = [0u8; 4];
    socket.read_exact(&mut len_buf).await?;
    let header_len = u32::from_be_bytes(len_buf) as usize;
    let mut header_buf = vec![0u8; header_len];
    socket.read_exact(&mut header_buf).await?;
    let header: Value = serde_json::from_slice(&header_buf)?;

    let file_name = header
        .get("fileName")
        .and_then(|v| v.as_str())
        .unwrap_or("archivo")
        .to_string();
    let dest_path = unique_dest_path(&state.received_dir, &file_name);

    let mut file = tokio::fs::File::create(&dest_path).await?;
    let mut buf = [0u8; 65536];
    loop {
        let n = socket.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).await?;
    }
    file.flush().await?;

    let from_id = header
        .get("fromId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let from_name = header
        .get("fromName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let msg = json!({
        "fromMe": false,
        "type": "file",
        "fileName": file_name,
        "filePath": dest_path.to_string_lossy(),
        "fileSize": header.get("fileSize").cloned().unwrap_or(Value::Null),
        "mimeType": header.get("mimeType").cloned().unwrap_or(Value::Null),
        "timestamp": header.get("timestamp").cloned().unwrap_or(json!(now_ms())),
    });
    windows::handle_incoming_message(&app, &state, &from_id, &from_name, msg);
    Ok(())
}

pub struct OutgoingFileMeta {
    pub file_path: String,
    pub file_name: String,
    pub file_size: u64,
    pub mime_type: String,
    pub timestamp: i64,
}

/// Manda un archivo a un peer por TCP, avisando el progreso a la ventana de
/// chat vía eventos ('file-send-progress'). Devuelve true si se mandó ok.
pub async fn send_file_to_peer(
    app: &AppHandle,
    state: &Arc<AppState>,
    peer_ip_addr: &str,
    meta: &OutgoingFileMeta,
    temp_id: &str,
) -> bool {
    let my_id = state.my_id.clone();
    let my_username = state.my_username.lock().unwrap().clone();

    let stream = match TcpStream::connect((peer_ip_addr, FILE_PORT)).await {
        Ok(s) => s,
        Err(_) => return false,
    };
    let mut stream = stream;

    let header = json!({
        "fromId": my_id,
        "fromName": my_username,
        "fileName": meta.file_name,
        "fileSize": meta.file_size,
        "mimeType": meta.mime_type,
        "timestamp": meta.timestamp,
    });
    let header_bytes = serde_json::to_vec(&header).unwrap_or_default();
    let len_bytes = (header_bytes.len() as u32).to_be_bytes();

    if stream.write_all(&len_bytes).await.is_err() {
        return false;
    }
    if stream.write_all(&header_bytes).await.is_err() {
        return false;
    }

    let mut file = match tokio::fs::File::open(&meta.file_path).await {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut buf = vec![0u8; 65536];
    let mut sent: u64 = 0;
    let mut last_percent: i64 = -1;
    loop {
        let n = match file.read(&mut buf).await {
            Ok(n) => n,
            Err(_) => return false,
        };
        if n == 0 {
            break;
        }
        if stream.write_all(&buf[..n]).await.is_err() {
            return false;
        }
        sent += n as u64;
        let percent = if meta.file_size > 0 {
            ((sent as f64 / meta.file_size as f64) * 100.0).round().min(100.0) as i64
        } else {
            100
        };
        if percent != last_percent {
            last_percent = percent;
            let _ = app.emit_to(
                "chat",
                "file-send-progress",
                json!({ "tempId": temp_id, "percent": percent, "sentBytes": sent, "fileSize": meta.file_size }),
            );
        }
    }
    let _ = stream.shutdown().await;
    true
}
