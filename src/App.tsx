import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import Login from "./components/Login";
import Panel from "./components/Panel";
import Chat from "./components/Chat";
import QuickReply from "./components/QuickReply";

// Las cuatro ventanas de la app (login, panel, chat, quickreply) cargan el
// mismo bundle de React; esta función decide qué pantalla mostrar según la
// etiqueta ("label") de la ventana de Tauri actual — el equivalente a que
// cada BrowserWindow de Electron cargara un .html distinto.
export default function App() {
  const [label, setLabel] = useState<string | null>(null);

  useEffect(() => {
    setLabel(getCurrentWindow().label);
  }, []);

  if (label === null) return null;

  switch (label) {
    case "login":
      return <Login />;
    case "panel":
      return <Panel />;
    case "chat":
      return <Chat />;
    case "quickreply":
      return <QuickReply />;
    default:
      return <Panel />;
  }
}
