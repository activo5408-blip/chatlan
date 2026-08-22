import { FormEvent, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { chatAPI } from "../lib/api";

export default function Login() {
  const [username, setUsername] = useState("");
  const [submitting, setSubmitting] = useState(false);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    const name = username.trim();
    if (!name || submitting) return;
    setSubmitting(true);
    try {
      await chatAPI.setUsername(name);
      // La ventana de login se cierra desde el lado de Rust apenas se
      // confirma el nombre (ver commands::set_username).
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="window-frame">
      <div className="flyout-header" data-tauri-drag-region>
        <span className="title">ChatLAN</span>
        <button
          className="close-btn"
          aria-label="Cerrar"
          onClick={() => getCurrentWindow().close()}
        >
          ✕
        </button>
      </div>

      <div className="login-body">
        <div className="login-card">
          <div className="login-mark">
            <span className="dot" />
            <span className="dot" />
            <span className="dot" />
          </div>
          <h1>ChatLAN</h1>
          <p className="login-sub">
            Chat P2P para esta red local. Se anuncia sola, sin servidor.
          </p>
          <form onSubmit={handleSubmit}>
            <input
              type="text"
              placeholder="¿Cómo te llamás?"
              maxLength={40}
              autoComplete="off"
              required
              value={username}
              onChange={(e) => setUsername(e.target.value)}
            />
            <button type="submit" disabled={submitting}>
              Entrar a la red
            </button>
          </form>
        </div>
      </div>
    </div>
  );
}
