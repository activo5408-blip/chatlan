import { FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import {
  chatAPI,
  ChatMessage,
  FileSendProgressData,
  FileSendStartData,
  PeerClient,
  formatBytes,
  toFileUrl,
} from "../lib/api";

const EMOJIS = [
  "😀", "😁", "😂", "🤣", "😊", "😉", "😍", "😘", "😜", "🤔",
  "😎", "🙂", "😴", "😢", "😭", "😡", "😱", "🥳", "🤗", "🤩",
  "👍", "👎", "👏", "🙌", "🙏", "💪", "👋", "✌️", "🤝", "❤️",
  "🔥", "🎉", "✅", "❌", "⭐", "💯", "☕", "🍕", "🎮", "📌",
];

const INPUT_MAX_HEIGHT = 110;

type PendingUpload = FileSendStartData & { percent: number; sentBytes: number };

export default function Chat() {
  const [peer, setPeer] = useState<PeerClient | null>(null);
  const [history, setHistory] = useState<ChatMessage[]>([]);
  const [peerOnline, setPeerOnline] = useState(false);
  const [typingActive, setTypingActive] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [emojiOpen, setEmojiOpen] = useState(false);
  const [attaching, setAttaching] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const [pendingUploads, setPendingUploads] = useState<Map<string, PendingUpload>>(new Map());
  const [text, setText] = useState("");

  const currentPeerId = peer?.id ?? null;
  const currentPeerIdRef = useRef<string | null>(null);
  currentPeerIdRef.current = currentPeerId;

  const messagesRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const typingClearTimer = useRef<number | null>(null);
  const typingSendTimer = useRef<number | null>(null);
  const dragDepth = useRef(0);

  // ---------------- Suscripciones a eventos del backend ----------------
  useEffect(() => {
    const unlistens = [
      chatAPI.onActivePeer((data) => {
        setPeer(data.peer);
        setHistory(data.history || []);
        setPeerOnline(data.peer.online);
        clearTyping();
        setText("");
        setEmojiOpen(false);
        setNotice(null);
        setTimeout(() => inputRef.current?.focus(), 0);
      }),
      chatAPI.onSeenUpdated((data) => {
        if (data.peerId !== currentPeerIdRef.current) return;
        setHistory((prev) =>
          prev.map((m) => (m.fromMe && !m.seen && m.timestamp <= data.upto ? { ...m, seen: true } : m))
        );
      }),
      chatAPI.onPeersUpdated((list) => {
        if (!currentPeerIdRef.current) return;
        const found = list.find((p) => p.id === currentPeerIdRef.current);
        if (found) {
          setPeer(found);
          setPeerOnline(found.online);
        } else {
          setPeerOnline(false);
        }
      }),
      chatAPI.onMessageReceived((msg) => {
        setHistory((prev) => [...prev, msg]);
      }),
      chatAPI.onTypingReceived((data) => {
        if (data.peerId === currentPeerIdRef.current) showTyping();
      }),
      chatAPI.onFileSendStart((data) => {
        if (data.peerId !== currentPeerIdRef.current) return;
        setPendingUploads((prev) => new Map(prev).set(data.tempId, { ...data, percent: 0, sentBytes: 0 }));
      }),
      chatAPI.onFileSendProgress((data: FileSendProgressData) => {
        setPendingUploads((prev) => {
          if (!prev.has(data.tempId)) return prev;
          const next = new Map(prev);
          const item = next.get(data.tempId)!;
          next.set(data.tempId, { ...item, percent: data.percent, sentBytes: data.sentBytes });
          return next;
        });
      }),
      chatAPI.onFileSendError((data) => {
        setPendingUploads((prev) => {
          if (!prev.has(data.tempId)) return prev;
          const next = new Map(prev);
          next.delete(data.tempId);
          return next;
        });
        setNotice("No se pudo enviar el archivo. ¿La otra persona sigue conectada?");
      }),
    ];
    return () => {
      unlistens.forEach((p) => p.then((fn) => fn()));
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ---------------- Arrastrar y soltar (drag & drop nativo de Tauri) ----------------
  useEffect(() => {
    const webview = getCurrentWebviewWindow();
    const unlistenPromise = webview.onDragDropEvent((event) => {
      const payload = event.payload as any;
      if (payload.type === "enter" || payload.type === "over") {
        if (currentPeerIdRef.current) setDragOver(true);
      } else if (payload.type === "leave") {
        setDragOver(false);
      } else if (payload.type === "drop") {
        setDragOver(false);
        const peerId = currentPeerIdRef.current;
        const paths: string[] = payload.paths || [];
        if (!peerId || paths.length === 0) return;
        (async () => {
          for (const path of paths) {
            await finalizeFileResult(chatAPI.sendFilePath(peerId, path), peerId);
          }
        })();
      }
    });
    return () => {
      unlistenPromise.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ---------------- Pegar una foto copiada (Ctrl+V) ----------------
  useEffect(() => {
    function onPaste(e: ClipboardEvent) {
      const peerId = currentPeerIdRef.current;
      if (!peerId) return;
      const items = Array.from(e.clipboardData?.items || []);
      const hasImage = items.some((it) => it.type && it.type.startsWith("image/"));
      if (!hasImage) return;
      e.preventDefault();
      finalizeFileResult(chatAPI.sendClipboardImage(peerId), peerId);
    }
    window.addEventListener("paste", onPaste);
    return () => window.removeEventListener("paste", onPaste);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    messagesRef.current?.scrollTo({ top: messagesRef.current.scrollHeight });
  }, [history, pendingUploads]);

  useEffect(() => {
    autoResizeInput();
  }, [text]);

  function clearTyping() {
    setTypingActive(false);
    if (typingClearTimer.current) window.clearTimeout(typingClearTimer.current);
    typingClearTimer.current = null;
  }

  function showTyping() {
    setTypingActive(true);
    if (typingClearTimer.current) window.clearTimeout(typingClearTimer.current);
    typingClearTimer.current = window.setTimeout(() => setTypingActive(false), 3000);
  }

  async function finalizeFileResult(promise: Promise<any>, forPeerId: string) {
    const result = await promise;
    if (result && result.tempId) {
      setPendingUploads((prev) => {
        if (!prev.has(result.tempId)) return prev;
        const next = new Map(prev);
        next.delete(result.tempId);
        return next;
      });
    }
    // Si mientras se mandaba el archivo el usuario se cambió a otra
    // conversación, no lo mezclamos con lo que se está viendo ahora: ya
    // quedó guardado en su historial y va a aparecer al reabrir ese chat.
    if (forPeerId !== currentPeerIdRef.current) return;
    if (result && result.error) {
      if (!result.tempId) {
        setNotice(
          result.empty
            ? "No hay ninguna imagen copiada en el portapapeles."
            : "No se pudo enviar el archivo. ¿La otra persona sigue conectada?"
        );
      }
      return;
    }
    if (result) {
      setHistory((prev) => [...prev, result]);
    }
  }

  async function handleAttachClick() {
    if (!currentPeerId) return;
    const peerId = currentPeerId;
    setAttaching(true);
    try {
      const result = await chatAPI.sendFile(peerId);
      if (result) await finalizeFileResult(Promise.resolve(result), peerId);
    } finally {
      setAttaching(false);
    }
  }

  function autoResizeInput() {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = "auto";
    const next = Math.min(el.scrollHeight, INPUT_MAX_HEIGHT);
    el.style.height = next + "px";
    el.style.overflowY = el.scrollHeight > INPUT_MAX_HEIGHT ? "auto" : "hidden";
  }

  function handleInputChange(value: string) {
    setText(value);
    if (!currentPeerId) return;
    if (typingSendTimer.current) return;
    chatAPI.sendTyping(currentPeerId);
    typingSendTimer.current = window.setTimeout(() => {
      typingSendTimer.current = null;
    }, 2000);
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      (e.target as HTMLTextAreaElement).form?.requestSubmit();
    }
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    const value = text.trim();
    if (!value || !currentPeerId) return;
    setText("");
    setEmojiOpen(false);
    // No agregamos el mensaje "a mano": el backend ya nos lo reenvía por
    // 'message-received' en cuanto lo procesa (ver onMessageReceived).
    const result = await chatAPI.sendMessage(currentPeerId, value);
    if (!result || !result.ok) {
      setNotice("No se pudo enviar el mensaje. ¿La otra persona sigue conectada?");
    }
  }

  function insertEmoji(emoji: string) {
    const el = inputRef.current;
    if (!el) {
      setText((t) => t + emoji);
      return;
    }
    const start = el.selectionStart ?? text.length;
    const end = el.selectionEnd ?? text.length;
    const next = text.slice(0, start) + emoji + text.slice(end);
    setText(next);
    requestAnimationFrame(() => {
      el.focus();
      const pos = start + emoji.length;
      el.setSelectionRange(pos, pos);
    });
  }

  const statusText = useMemo(() => {
    if (typingActive) return "Escribiendo…";
    return peerOnline ? "En línea" : "Desconectado";
  }, [typingActive, peerOnline]);
  const statusClass = typingActive ? "typing" : peerOnline ? "online" : "offline";

  const uploadsForPeer = useMemo(
    () => Array.from(pendingUploads.values()).filter((u) => u.peerId === currentPeerId),
    [pendingUploads, currentPeerId]
  );

  return (
    <div className="window-frame">
      <div className="flyout-header" data-tauri-drag-region>
        <button className="back-btn" aria-label="Volver a contactos" onClick={() => chatAPI.backToPanel()}>
          ←
        </button>
        <span className="title chat-header-info">
          <span
            className="avatar"
            style={{ width: 22, height: 22, ...(peer?.avatar ? { backgroundImage: `url(${peer.avatar})` } : {}) }}
          />
          <span>
            <span className="chat-name" style={{ display: "block" }}>
              {peer?.username || ""}
            </span>
            <span className={"chat-status " + statusClass} style={{ display: "block" }}>
              {statusText}
            </span>
          </span>
        </span>
        <button className="close-btn" aria-label="Ocultar" onClick={() => chatAPI.hideWindow()}>
          ✕
        </button>
      </div>

      <div className="messages" ref={messagesRef}>
        {history.map((m, i) => (
          <MessageRow key={i} msg={m} />
        ))}
        {notice && <div className="system-notice">{notice}</div>}
        {uploadsForPeer.map((item) => (
          <PendingRow key={item.tempId} item={item} />
        ))}
      </div>

      <div className={"drop-overlay" + (dragOver ? "" : " hidden")}>Soltá acá para enviar</div>

      <div className={"emoji-panel" + (emojiOpen ? "" : " hidden")}>
        {EMOJIS.map((emoji) => (
          <button
            key={emoji}
            type="button"
            className="emoji-option"
            onClick={() => insertEmoji(emoji)}
          >
            {emoji}
          </button>
        ))}
      </div>

      <form className="message-form" onSubmit={handleSubmit}>
        <button
          type="button"
          className="icon-btn"
          aria-label="Emojis"
          onClick={() => setEmojiOpen((v) => !v)}
        >
          😊
        </button>
        <button
          type="button"
          className="icon-btn"
          aria-label="Adjuntar archivo o foto"
          disabled={attaching}
          onClick={handleAttachClick}
        >
          {attaching ? "…" : "📎"}
        </button>
        <textarea
          ref={inputRef}
          rows={1}
          placeholder="Escribí un mensaje…"
          value={text}
          onChange={(e) => handleInputChange(e.target.value)}
          onKeyDown={handleKeyDown}
        />
        <button type="submit" aria-label="Enviar">
          ➤
        </button>
      </form>
    </div>
  );
}

function MessageRow({ msg }: { msg: ChatMessage }) {
  const time = new Date(msg.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return (
    <div className={"msg-row " + (msg.fromMe ? "me" : "them")}>
      <div className="bubble">
        {msg.type === "file" ? (
          msg.mimeType?.startsWith("image/") ? (
            <img
              className="chat-image"
              src={toFileUrl(msg.filePath || "")}
              alt={msg.fileName}
              onClick={() => msg.filePath && chatAPI.openFile(msg.filePath)}
            />
          ) : (
            <div className="file-card" onClick={() => msg.filePath && chatAPI.openFile(msg.filePath)}>
              <span className="file-icon">📄</span>
              <span className="file-info">
                <span className="file-name">{msg.fileName}</span>
                <span className="file-size">{formatBytes(msg.fileSize)}</span>
              </span>
            </div>
          )
        ) : (
          <span>{msg.text}</span>
        )}
        <span className="msg-meta">
          <span className="time">{time}</span>
          {msg.fromMe && (
            <span className={"msg-status" + (msg.seen ? " seen" : "")} title={msg.seen ? "Visto" : "Enviado"}>
              {msg.seen ? "✓✓" : "✓"}
            </span>
          )}
        </span>
      </div>
    </div>
  );
}

function PendingRow({ item }: { item: PendingUpload }) {
  const isImage = item.mimeType?.startsWith("image/");
  return (
    <div className="msg-row me">
      <div className="bubble">
        <div className="file-card">
          <span className="file-icon">{isImage ? "🖼️" : "📄"}</span>
          <span className="file-info">
            <span className="file-name">{item.fileName}</span>
          </span>
        </div>
        <div className="upload-info">
          Enviando… {formatBytes(item.sentBytes || 0)} / {formatBytes(item.fileSize)} · {item.percent || 0}%
        </div>
        <div className="upload-progress">
          <div className="upload-progress-fill" style={{ width: (item.percent || 0) + "%" }} />
        </div>
      </div>
    </div>
  );
}
