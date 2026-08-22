import { useEffect, useState } from "react";
import { chatAPI, PeerClient } from "../lib/api";

export default function Panel() {
  const [myName, setMyName] = useState("");
  const [myAvatar, setMyAvatar] = useState<string | null>(null);
  const [peers, setPeers] = useState<PeerClient[]>([]);
  const [autostart, setAutostart] = useState(false);
  const [pickingAvatar, setPickingAvatar] = useState(false);

  useEffect(() => {
    chatAPI.getInit().then((data) => {
      setMyName(data.username || "");
      setMyAvatar(data.avatar);
      setPeers(data.peers || []);
    });
    chatAPI.getAutostart().then(setAutostart);

    const unlisten = chatAPI.onPeersUpdated((list) => setPeers(list));
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  async function handleAvatarClick() {
    setPickingAvatar(true);
    try {
      const result = await chatAPI.pickAvatar();
      if (result && "avatar" in result && result.avatar) {
        setMyAvatar(result.avatar);
      }
    } finally {
      setPickingAvatar(false);
    }
  }

  async function handleAutostartChange(checked: boolean) {
    setAutostart(checked);
    await chatAPI.setAutostart(checked);
  }

  const sorted = [...peers].sort((a, b) => a.username.localeCompare(b.username));

  return (
    <div className="window-frame">
      <div className="flyout-header" data-tauri-drag-region>
        <span className="title">ChatLAN</span>
        <button className="close-btn" aria-label="Ocultar" onClick={() => chatAPI.hideWindow()}>
          ✕
        </button>
      </div>

      <div className="me-strip">
        <button
          type="button"
          className="avatar avatar-btn"
          title="Cambiar foto de perfil"
          aria-label="Cambiar foto de perfil"
          disabled={pickingAvatar}
          style={myAvatar ? { backgroundImage: `url(${myAvatar})` } : undefined}
          onClick={handleAvatarClick}
        />
        <div>
          <div className="me-name">{myName}</div>
          <div className="me-tag">Vos</div>
        </div>
      </div>

      <div className="section-title">
        EN LA RED <span>({sorted.length})</span>
      </div>
      <div className="peer-list">
        {sorted.length === 0 && (
          <div className="empty-peers">Buscando otras PCs en la red…</div>
        )}
        {sorted.map((peer) => (
          <div key={peer.id} className="peer-item" onClick={() => chatAPI.openChat(peer.id)}>
            <span className="avatar-wrap">
              <span
                className="avatar"
                style={peer.avatar ? { backgroundImage: `url(${peer.avatar})` } : undefined}
              />
              <span className={"status-dot" + (peer.online ? " online" : "")} />
            </span>
            <span className="peer-info">
              <div className="peer-name">{peer.username}</div>
              <div className="peer-sub">{peer.online ? "En línea" : "Desconectado"}</div>
            </span>
            {peer.unread > 0 && <span className="peer-badge">{peer.unread}</span>}
          </div>
        ))}
      </div>

      <label className="settings-row">
        <input
          type="checkbox"
          checked={autostart}
          onChange={(e) => handleAutostartChange(e.target.checked)}
        />
        Iniciar automáticamente con Windows
      </label>
    </div>
  );
}
