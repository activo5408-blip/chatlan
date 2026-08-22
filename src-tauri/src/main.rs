// ChatLAN (Tauri + React)
// P2P por UDP broadcast. La app vive en la bandeja: un clic abre el panel
// de contactos; al elegir uno, se abre la ventana de chat con esa persona.
// Los mensajes nuevos disparan una ventanita de respuesta rápida + sonido.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod net;
mod state;
mod windows;

use state::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{Manager, WindowEvent};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Alguien intentó abrir otro ChatLAN: en vez de un segundo ícono,
            // mostramos el panel (o el login) de la instancia que ya corre.
            let state = app.state::<Arc<AppState>>();
            if state.windows_created.load(Ordering::SeqCst) {
                windows::show_panel_window(app, &state);
            } else if let Some(login) = app.get_webview_window("login") {
                let _ = login.unminimize();
                let _ = login.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .setup(|app| {
            let handle = app.handle().clone();

            let app_data_dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir());
            let documents_dir = app.path().document_dir().unwrap_or_else(|_| std::env::temp_dir());
            let state = Arc::new(AppState::new(app_data_dir, documents_dir));
            app.manage(state.clone());

            // Aplica el ajuste de inicio automático guardado (por defecto: sí).
            commands::apply_autostart_setting(&handle, &state, state.settings.lock().unwrap().auto_launch);

            // Red: socket UDP de descubrimiento/mensajería + servidor TCP de archivos.
            let handle2 = handle.clone();
            let state2 = state.clone();
            tauri::async_runtime::spawn(async move {
                match net::setup_udp_socket().await {
                    Ok(socket) => {
                        *state2.udp_socket.lock().await = Some(socket.clone());
                        net::start_udp_receive_loop(handle2.clone(), state2.clone(), socket.clone());
                        net::spawn_presence_task(state2.clone(), socket);
                        net::spawn_cleanup_task(handle2.clone(), state2.clone());
                        net::start_file_server(handle2, state2);
                    }
                    Err(e) => eprintln!("No se pudo abrir el socket UDP ({}): {e}", state::DISCOVERY_PORT),
                }
            });

            windows::create_login_window(&handle)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<Arc<AppState>>();
                let label = window.label();

                if label == "login" {
                    // Si cierran el login sin haber elegido nombre, la app entera
                    // se cierra (todavía no hay bandeja ni ventanas de fondo).
                    if !state.windows_created.load(Ordering::SeqCst) {
                        app.exit(0);
                    }
                    return;
                }

                // El panel, el chat y la respuesta rápida no se cierran nunca de
                // verdad (salvo que estemos saliendo de la app): se ocultan, para
                // que la app siga viva en la bandeja.
                if !state.is_quitting.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                    if label == "chat" {
                        *state.active_peer_id.lock().unwrap() = None;
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_init,
            commands::set_username,
            commands::send_message,
            commands::open_chat,
            commands::back_to_panel,
            commands::hide_window,
            commands::get_autostart,
            commands::set_autostart,
            commands::send_file,
            commands::send_file_path,
            commands::send_clipboard_image,
            commands::pick_avatar,
            commands::send_typing,
            commands::open_file,
        ])
        .build(tauri::generate_context!())
        .expect("error construyendo la app de ChatLAN")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = event {
                let state = app_handle.state::<Arc<AppState>>();
                state.is_quitting.store(true, Ordering::SeqCst);
                let state = state.inner().clone();
                tauri::async_runtime::block_on(async move {
                    if let Some(socket) = state.udp_socket.lock().await.clone() {
                        net::send_bye_broadcast(&state, &socket).await;
                    }
                });
            }
        });
}
