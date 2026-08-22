use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::Mutex as AsyncMutex;

pub const DISCOVERY_PORT: u16 = 41234;
pub const FILE_PORT: u16 = 41235;
pub const PRESENCE_INTERVAL_MS: u64 = 2500;
pub const PEER_TIMEOUT_MS: i64 = 8000;
pub const CLEANUP_INTERVAL_MS: u64 = 3000;
pub const HISTORY_LIMIT_PER_CONTACT: usize = 500;

pub const QUICK_REPLY_WIDTH: f64 = 320.0;
pub const QUICK_REPLY_HEIGHT: f64 = 168.0;
pub const PANEL_WIDTH: f64 = 300.0;
pub const PANEL_HEIGHT: f64 = 460.0;
pub const CHAT_WIDTH: f64 = 320.0;
pub const CHAT_HEIGHT: f64 = 480.0;

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[derive(Clone, Debug)]
pub struct PeerInfo {
    pub id: String,
    pub username: String,
    pub ip: String,
    pub last_seen: i64,
    pub avatar: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SettingsData {
    #[serde(rename = "autoLaunch", default = "default_true")]
    pub auto_launch: bool,
    #[serde(rename = "myAvatar", skip_serializing_if = "Option::is_none")]
    pub my_avatar: Option<String>,
}
fn default_true() -> bool {
    true
}

pub struct AppState {
    pub my_id: String,
    pub my_username: Mutex<Option<String>>,
    pub peers: Mutex<HashMap<String, PeerInfo>>,
    pub unread_counts: Mutex<HashMap<String, u32>>,
    pub conversations: Mutex<HashMap<String, Vec<Value>>>,
    pub history_store: Mutex<HashMap<String, Vec<Value>>>,
    pub settings: Mutex<SettingsData>,
    pub active_peer_id: Mutex<Option<String>>,
    pub windows_created: AtomicBool,
    pub is_quitting: AtomicBool,
    pub last_presence_at: AtomicI64,
    pub history_path: PathBuf,
    pub settings_path: PathBuf,
    pub received_dir: PathBuf,
    pub sent_dir: PathBuf,
    pub udp_socket: AsyncMutex<Option<std::sync::Arc<UdpSocket>>>,
}

impl AppState {
    pub fn new(app_data_dir: PathBuf, documents_dir: PathBuf) -> Self {
        let history_path = app_data_dir.join("history.json");
        let settings_path = app_data_dir.join("settings.json");
        let received_dir = documents_dir.join("ChatLAN").join("Recibidos");
        let sent_dir = documents_dir.join("ChatLAN").join("Enviados");
        let _ = std::fs::create_dir_all(&app_data_dir);
        let _ = std::fs::create_dir_all(&received_dir);
        let _ = std::fs::create_dir_all(&sent_dir);

        let history_store = load_json_map(&history_path);
        let mut settings: SettingsData = std::fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        // Por defecto: iniciar con Windows, igual que la versión Electron.
        if !settings_path.exists() {
            settings.auto_launch = true;
            let _ = std::fs::write(&settings_path, serde_json::to_string(&settings).unwrap());
        }

        AppState {
            my_id: uuid::Uuid::new_v4().to_string(),
            my_username: Mutex::new(None),
            peers: Mutex::new(HashMap::new()),
            unread_counts: Mutex::new(HashMap::new()),
            conversations: Mutex::new(HashMap::new()),
            history_store: Mutex::new(history_store),
            settings: Mutex::new(settings),
            active_peer_id: Mutex::new(None),
            windows_created: AtomicBool::new(false),
            is_quitting: AtomicBool::new(false),
            last_presence_at: AtomicI64::new(0),
            history_path,
            settings_path,
            received_dir,
            sent_dir,
            udp_socket: AsyncMutex::new(None),
        }
    }

    pub fn save_history(&self) {
        let store = self.history_store.lock().unwrap();
        let _ = std::fs::write(&self.history_path, serde_json::to_string(&*store).unwrap_or_default());
    }

    pub fn save_settings(&self) {
        let settings = self.settings.lock().unwrap();
        let _ = std::fs::write(&self.settings_path, serde_json::to_string(&*settings).unwrap_or_default());
    }

    pub fn username_key_for(username: &str) -> String {
        username.trim().to_lowercase()
    }

    /// Trae (o inicializa desde el historial persistido) la conversación en memoria de un peer.
    pub fn get_conversation(&self, peer_id: &str, username: &str) -> Vec<Value> {
        let mut convs = self.conversations.lock().unwrap();
        if !convs.contains_key(peer_id) {
            let history = self.history_store.lock().unwrap();
            let persisted = history
                .get(&Self::username_key_for(username))
                .cloned()
                .unwrap_or_default();
            convs.insert(peer_id.to_string(), persisted);
        }
        convs.get(peer_id).cloned().unwrap_or_default()
    }

    pub fn append_to_conversation(&self, peer_id: &str, username: &str, msg: Value) {
        {
            let mut convs = self.conversations.lock().unwrap();
            let persisted_seed = {
                let history = self.history_store.lock().unwrap();
                history
                    .get(&Self::username_key_for(username))
                    .cloned()
                    .unwrap_or_default()
            };
            let entry = convs
                .entry(peer_id.to_string())
                .or_insert(persisted_seed);
            entry.push(msg.clone());
        }
        let key = Self::username_key_for(username);
        let mut history = self.history_store.lock().unwrap();
        let entry = history.entry(key).or_insert_with(Vec::new);
        entry.push(msg);
        if entry.len() > HISTORY_LIMIT_PER_CONTACT {
            let excess = entry.len() - HISTORY_LIMIT_PER_CONTACT;
            entry.drain(0..excess);
        }
        drop(history);
        self.save_history();
    }

    /// Marca como "vistos" los mensajes que YO mandé a peer_id hasta upto (ms).
    /// Devuelve true si cambió algo.
    pub fn mark_messages_seen_by(&self, peer_id: &str, upto: i64) -> bool {
        let username = self
            .peers
            .lock()
            .unwrap()
            .get(peer_id)
            .map(|p| p.username.clone())
            .unwrap_or_default();
        let mut changed = false;
        {
            let mut convs = self.conversations.lock().unwrap();
            if let Some(conv) = convs.get_mut(peer_id) {
                for m in conv.iter_mut() {
                    mark_seen_if_needed(m, upto, &mut changed);
                }
            }
        }
        if changed {
            let key = Self::username_key_for(&username);
            let mut history = self.history_store.lock().unwrap();
            if let Some(entry) = history.get_mut(&key) {
                for m in entry.iter_mut() {
                    let mut c = false;
                    mark_seen_if_needed(m, upto, &mut c);
                }
            }
            drop(history);
            self.save_history();
        }
        changed
    }

    pub fn peer_list_for_client(&self) -> Vec<Value> {
        let peers = self.peers.lock().unwrap();
        let unread = self.unread_counts.lock().unwrap();
        let now = now_ms();
        peers
            .values()
            .map(|p| {
                json!({
                    "id": p.id,
                    "username": p.username,
                    "online": now - p.last_seen < PEER_TIMEOUT_MS,
                    "unread": unread.get(&p.id).copied().unwrap_or(0),
                    "avatar": p.avatar,
                })
            })
            .collect()
    }
}

fn mark_seen_if_needed(m: &mut Value, upto: i64, changed: &mut bool) {
    if let Value::Object(obj) = m {
        let from_me = obj.get("fromMe").and_then(|v| v.as_bool()).unwrap_or(false);
        let seen = obj.get("seen").and_then(|v| v.as_bool()).unwrap_or(false);
        let ts = obj.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
        if from_me && !seen && ts <= upto {
            obj.insert("seen".to_string(), Value::Bool(true));
            *changed = true;
        }
    }
}

fn load_json_map(path: &PathBuf) -> HashMap<String, Vec<Value>> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn guess_mime(file_name: &str) -> String {
    let ext = file_name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_string()
}

pub fn unique_dest_path(dir: &PathBuf, file_name: &str) -> PathBuf {
    let safe_name = sanitize_filename(file_name);
    let mut candidate = dir.join(&safe_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = std::path::Path::new(&safe_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "archivo".to_string());
    let ext = path
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let mut i = 1;
    loop {
        candidate = dir.join(format!("{} ({}){}", stem, i, ext));
        if !candidate.exists() {
            return candidate;
        }
        i += 1;
    }
}

fn sanitize_filename(name: &str) -> String {
    let base = std::path::Path::new(name)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let cleaned: String = base
        .chars()
        .map(|c| {
            if r#"<>:"/\|?*"#.contains(c) || (c as u32) < 0x20 {
                '_'
            } else {
                c
            }
        })
        .collect();
    if cleaned.trim().is_empty() {
        "archivo".to_string()
    } else {
        cleaned
    }
}

pub fn make_object(pairs: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (k, v) in pairs {
        map.insert(k.to_string(), v);
    }
    Value::Object(map)
}
