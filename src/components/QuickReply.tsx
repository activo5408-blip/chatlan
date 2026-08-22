import { FormEvent, useEffect, useRef, useState } from "react";
import { chatAPI, PeerClient } from "../lib/api";

export default function QuickReply() {
  const [peer, setPeer] = useState<PeerClient | null>(null);
  const [preview, setPreview] = useState("");
  const [text, setText] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const unlisten = chatAPI.onQuickReply((data) => {
      setPeer(data.peer);
      setPreview(data.preview || "");
      setText("");
      // Pequeño delay para que el foco entre después de que la ventana
      // termine de mostrarse, igual que en la versión Electron.
      setTimeout(() => inputRef.current?.focus(), 30);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    const value = text.trim();
    if (!value || !peer) return;
    setText("");
    await chatAPI.sendMessage(peer.id, value);
    chatAPI.hideWindow();
  }

  function handleHeadClick(e: React.MouseEvent) {
    if ((e.target as HTMLElement).closest(".close-btn")) return;
    if (peer) chatAPI.openChat(peer.id);
  }

  return (
    <div className="window-frame quickreply-body">
      <div className="qr-card">
        <div className="qr-head" onClick={handleHeadClick}>
          <span
            className="avatar"
            style={{
              width: 30,
              height: 30,
              ...(peer?.avatar ? { backgroundImage: `url(${peer.avatar})` } : {}),
            }}
          />
          <span className="qr-head-text">
            <span className="qr-name">{peer?.username || ""}</span>
            <span className="qr-preview">{preview}</span>
          </span>
          <button className="close-btn" aria-label="Cerrar" onClick={() => chatAPI.hideWindow()}>
            ✕
          </button>
        </div>
        <form className="qr-form" onSubmit={handleSubmit}>
          <input
            ref={inputRef}
            type="text"
            placeholder="Responder…"
            autoComplete="off"
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
          <button type="submit" className="qr-send" aria-label="Enviar">
            ➤
          </button>
        </form>
        <button
          type="button"
          className="qr-expand"
          onClick={() => peer && chatAPI.openChat(peer.id)}
        >
          Abrir conversación
        </button>
      </div>
    </div>
  );
}
