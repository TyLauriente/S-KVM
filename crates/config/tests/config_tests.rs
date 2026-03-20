//! Configuration tests.

use s_kvm_config::{AppConfig, load_config, save_config};

#[test]
fn default_config_has_valid_defaults() {
    let config = AppConfig::default();
    assert_eq!(config.network.listen_port, 24800);
    assert!(config.network.mdns_enabled);
    assert_eq!(config.network.mdns_service_type, "_softkvm._tcp.local.");
    assert!(config.input.raw_mouse_deltas);
    assert_eq!(config.input.edge_switch_delay_ms, 50);
    assert!(!config.video.enabled);
    assert_eq!(config.video.max_fps, 60);
    assert!(!config.audio.enabled);
    assert_eq!(config.audio.bitrate_kbps, 128);
    assert!(config.security.require_pairing);
    assert_eq!(config.hotkeys.toggle_active, "Ctrl+Alt+K");
    assert_eq!(config.hotkeys.switch_screen.len(), 4);
}

#[test]
fn config_toml_roundtrip() {
    let config = AppConfig::default();
    let toml_str = toml::to_string_pretty(&config).unwrap();
    let parsed: AppConfig = toml::from_str(&toml_str).unwrap();

    assert_eq!(parsed.network.listen_port, config.network.listen_port);
    assert_eq!(parsed.network.mdns_enabled, config.network.mdns_enabled);
    assert_eq!(parsed.input.raw_mouse_deltas, config.input.raw_mouse_deltas);
    assert_eq!(parsed.video.max_fps, config.video.max_fps);
    assert_eq!(parsed.audio.bitrate_kbps, config.audio.bitrate_kbps);
    assert_eq!(parsed.security.require_pairing, config.security.require_pairing);
}

#[test]
fn config_save_and_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let original = AppConfig::default();
    let content = toml::to_string_pretty(&original).unwrap();
    std::fs::write(&config_path, content).unwrap();

    let loaded: AppConfig = toml::from_str(
        &std::fs::read_to_string(&config_path).unwrap()
    ).unwrap();

    assert_eq!(loaded.network.listen_port, original.network.listen_port);
    assert_eq!(loaded.hotkeys.toggle_active, original.hotkeys.toggle_active);
}

#[test]
fn peer_id_is_unique_per_default() {
    let c1 = AppConfig::default();
    let c2 = AppConfig::default();
    assert_ne!(c1.peer_id.0, c2.peer_id.0);
}

#[test]
fn hostname_is_populated() {
    let config = AppConfig::default();
    assert!(!config.machine_name.is_empty());
    assert_ne!(config.machine_name, "unknown");
}
