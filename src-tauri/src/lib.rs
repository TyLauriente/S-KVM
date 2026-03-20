mod commands;
mod daemon_client;
mod state;

use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_peers,
            commands::get_connection_status,
            commands::connect_peer,
            commands::disconnect_peer,
            commands::get_displays,
            commands::update_screen_layout,
            commands::start_kvm,
            commands::stop_kvm,
            commands::get_kvm_status,
        ])
        .setup(|app| {
            tracing::info!("S-KVM application starting");

            // Initialize application state
            let state = app.state::<state::AppState>();
            let config = s_kvm_config::load_config()
                .unwrap_or_else(|_| s_kvm_config::AppConfig::default());
            *state.config.lock().unwrap() = config;

            // Try initial daemon connection
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = app_handle.state::<state::AppState>();
                match daemon_client::DaemonClient::connect().await {
                    Ok(client) => {
                        tracing::info!("Connected to S-KVM daemon");
                        *state.daemon_client.lock().await = Some(client);
                    }
                    Err(e) => {
                        tracing::info!("Daemon not available, running in standalone mode: {}", e);
                    }
                }
            });

            // Spawn reconnection task that retries every 5 seconds
            let reconnect_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                    let state = reconnect_handle.state::<state::AppState>();
                    let is_connected = state.daemon_client.lock().await.is_some();

                    if !is_connected {
                        match daemon_client::DaemonClient::connect().await {
                            Ok(client) => {
                                tracing::info!("Reconnected to S-KVM daemon");
                                *state.daemon_client.lock().await = Some(client);
                            }
                            Err(_) => {
                                tracing::trace!("Daemon still not available, will retry");
                            }
                        }
                    }
                }
            });

            // Set up system tray menu
            if let Some(tray) = app.tray_by_id("main") {
                let menu = tauri::menu::MenuBuilder::new(app)
                    .text("toggle", "Toggle KVM")
                    .separator()
                    .text("settings", "Settings")
                    .separator()
                    .text("quit", "Quit S-KVM")
                    .build()?;
                tray.set_menu(Some(menu))?;
                tray.set_tooltip(Some("S-KVM - Software KVM Switch"))?;

                let app_handle = app.handle().clone();
                tray.on_menu_event(move |_app, event| {
                    match event.id().as_ref() {
                        "toggle" => {
                            let state = app_handle.state::<state::AppState>();
                            let mut active = state.kvm_active.lock().unwrap();
                            *active = !*active;
                            tracing::info!(active = *active, "KVM toggled via tray");
                            let _ = app_handle.emit("kvm-status-changed", *active);
                        }
                        "settings" => {
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            tracing::info!("Quit requested via tray");
                            app_handle.exit(0);
                        }
                        _ => {}
                    }
                });
            }

            // Emit initial status
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Small delay to ensure frontend is ready
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let _ = handle.emit("kvm-status-changed", false);
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running S-KVM application");
}
