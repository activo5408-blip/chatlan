import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface PeerClient {
  id: string;
  username: string;
  online: boolean;
  unread: number;
  avatar: string | null;
}

export interface ChatMessage {
  fromMe: boolean;
  text?: string;
  timestamp: number;
  seen?: boolean;
  type?: "file";
  fileName?: string;
  filePath?: string;
  fileSize?: number;
  mimeType?: string;
  tempId?: string;
}

export interface InitData {
  myId: string;
  username: string | null;
  peers: PeerClient[];
  avatar: string | null;
}

export interface SendResult {
  ok: boolean;
  timestamp: number;
}

export interface FileSendStartData {
  tempId: string;
  peerId: string;
  fileName: string;
  fileSize: number;
  mimeType: string;
  timestamp: number;
}

export interface FileSendProgressData {
  tempId: string;
  percent: number;
  sentBytes: number;
  fileSize: number;
}

// ---------------------------------------------------------------------------
// Comandos (equivalentes a los ipcRenderer.invoke de preload.js)
// ---------------------------------------------------------------------------
export const chatAPI = {
  getInit: () => invoke<InitData>("get_init"),
  setUsername: (username: string) => invoke<{ myId: string; username: string }>("set_username", { username }),
  sendMessage: (toId: string, text: string) => invoke<SendResult>("send_message", { toId, text }),
  openChat: (peerId: string) => invoke<boolean>("open_chat", { peerId }),
  backToPanel: () => invoke<boolean>("back_to_panel"),
  hideWindow: () => invoke<boolean>("hide_window"),
  sendFile: (peerId: string) => invoke<ChatMessage | null>("send_file", { peerId }),
  sendFilePath: (peerId: string, filePath: string) =>
    invoke<ChatMessage | { error: true }>("send_file_path", { peerId, filePath }),
  sendClipboardImage: (peerId: string) =>
    invoke<ChatMessage | { error: true; empty?: boolean }>("send_clipboard_image", { peerId }),
  sendTyping: (peerId: string) => invoke<boolean>("send_typing", { peerId }),
  openFile: (filePath: string) => invoke<boolean>("open_file", { filePath }),
  getAutostart: () => invoke<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => invoke<boolean>("set_autostart", { enabled }),
  pickAvatar: () => invoke<{ avatar?: string; error?: true } | null>("pick_avatar"),

  // -------------------------------------------------------------------------
  // Eventos (equivalentes a los ipcRenderer.on de preload.js). Devuelven la
  // función de "unlisten" por si algún componente la necesita al desmontar.
  // -------------------------------------------------------------------------
  onPeersUpdated(cb: (peers: PeerClient[]) => void): Promise<UnlistenFn> {
    return listen<PeerClient[]>("peers-updated", (e) => cb(e.payload));
  },
  onMessageReceived(cb: (msg: ChatMessage) => void): Promise<UnlistenFn> {
    return listen<ChatMessage>("message-received", (e) => cb(e.payload));
  },
  onActivePeer(cb: (data: { peer: PeerClient; history: ChatMessage[] }) => void): Promise<UnlistenFn> {
    return listen("active-peer", (e) => cb(e.payload as any));
  },
  onTypingReceived(cb: (data: { peerId: string }) => void): Promise<UnlistenFn> {
    return listen("typing-received", (e) => cb(e.payload as any));
  },
  onSeenUpdated(cb: (data: { peerId: string; upto: number }) => void): Promise<UnlistenFn> {
    return listen("seen-updated", (e) => cb(e.payload as any));
  },
  onQuickReply(cb: (data: { peer: PeerClient; preview: string }) => void): Promise<UnlistenFn> {
    return listen("quick-reply-data", (e) => cb(e.payload as any));
  },
  onFileSendStart(cb: (data: FileSendStartData) => void): Promise<UnlistenFn> {
    return listen("file-send-start", (e) => cb(e.payload as any));
  },
  onFileSendProgress(cb: (data: FileSendProgressData) => void): Promise<UnlistenFn> {
    return listen("file-send-progress", (e) => cb(e.payload as any));
  },
  onFileSendError(cb: (data: { tempId: string }) => void): Promise<UnlistenFn> {
    return listen("file-send-error", (e) => cb(e.payload as any));
  },
};

export function formatBytes(bytes?: number): string {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  let n = bytes;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(n < 10 && i > 0 ? 1 : 0)} ${units[i]}`;
}

export function toFileUrl(p: string): string {
  // En Tauri, para mostrar una imagen del disco dentro del webview hace falta
  // pasarla por el protocolo especial de "asset" (habilitado en
  // tauri.conf.json -> app.security.assetProtocol). convertFileSrc arma la
  // URL correcta para cada plataforma.
  return convertFileSrc(p);
}
