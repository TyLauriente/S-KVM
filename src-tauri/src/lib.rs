mod commands;

use tauri::Manager;

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
            // Focus the main window if already running
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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

            // Set up system tray
            let _tray = app.tray_by_id("main");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running S-KVM application");
}
