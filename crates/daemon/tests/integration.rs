//! Integration tests for the S-KVM daemon coordinator and IPC types.

use std::sync::atomic::Ordering;

use s_kvm_core::ConnectionState;

use s_kvm_daemon::coordinator::{
    DaemonState, IpcCommand, IpcResponse, PeerStatusInfo,
};

// ── DaemonState tests ────────────────────────────────────────────────

#[tokio::test]
async fn daemon_state_tracks_peers() {
    let state = DaemonState::new();

    // Initially empty
    let peers = state.peers.lock().await;
    assert!(peers.is_empty());
    drop(peers);

    // Add a peer
    {
        let mut peers = state.peers.lock().await;
        peers.push(PeerStatusInfo {
            id: "peer-1".to_string(),
            hostname: "desktop".to_string(),
            os: "Linux".to_string(),
            state: ConnectionState::Active,
            latency_ms: Some(1.5),
        });
    }

    let peers = state.peers.lock().await;
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].id, "peer-1");
    assert_eq!(peers[0].hostname, "desktop");
    drop(peers);

    // Remove by retain
    {
        let mut peers = state.peers.lock().await;
        peers.retain(|p| p.id != "peer-1");
    }

    let peers = state.peers.lock().await;
    assert!(peers.is_empty());
}

#[tokio::test]
async fn daemon_state_kvm_toggle() {
    let state = DaemonState::new();

    assert!(!state.kvm_active.load(Ordering::Relaxed));

    state.kvm_active.store(true, Ordering::Relaxed);
    assert!(state.kvm_active.load(Ordering::Relaxed));

    state.kvm_active.store(false, Ordering::Relaxed);
    assert!(!state.kvm_active.load(Ordering::Relaxed));
}

#[test]
fn daemon_state_uptime() {
    let state = DaemonState::new();
    // Just verify start_time is recent (within 1 second)
    assert!(state.start_time.elapsed().as_secs() < 1);
}

// ── IPC command serialization roundtrips ─────────────────────────────

#[test]
fn ipc_command_get_status_roundtrip() {
    let cmd = IpcCommand::GetStatus;
    let json = serde_json::to_string(&cmd).unwrap();
    let decoded: IpcCommand = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, IpcCommand::GetStatus));
}

#[test]
fn ipc_command_get_peers_roundtrip() {
    let cmd = IpcCommand::GetPeers;
    let json = serde_json::to_string(&cmd).unwrap();
    let decoded: IpcCommand = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, IpcCommand::GetPeers));
}

#[test]
fn ipc_command_get_config_roundtrip() {
    let cmd = IpcCommand::GetConfig;
    let json = serde_json::to_string(&cmd).unwrap();
    let decoded: IpcCommand = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, IpcCommand::GetConfig));
}

#[test]
fn ipc_command_start_stop_kvm_roundtrip() {
    for cmd in [IpcCommand::StartKvm, IpcCommand::StopKvm] {
        let json = serde_json::to_string(&cmd).unwrap();
        let _decoded: IpcCommand = serde_json::from_str(&json).unwrap();
    }
}

#[test]
fn ipc_command_connect_peer_roundtrip() {
    let cmd = IpcCommand::ConnectPeer {
        address: "192.168.1.100".to_string(),
        port: 9876,
    };
    let json = serde_json::to_string(&cmd).unwrap();
    let decoded: IpcCommand = serde_json::from_str(&json).unwrap();
    match decoded {
        IpcCommand::ConnectPeer { address, port } => {
            assert_eq!(address, "192.168.1.100");
            assert_eq!(port, 9876);
        }
        _ => panic!("Expected ConnectPeer"),
    }
}

#[test]
fn ipc_command_disconnect_peer_roundtrip() {
    let cmd = IpcCommand::DisconnectPeer("abc-123".to_string());
    let json = serde_json::to_string(&cmd).unwrap();
    let decoded: IpcCommand = serde_json::from_str(&json).unwrap();
    match decoded {
        IpcCommand::DisconnectPeer(id) => assert_eq!(id, "abc-123"),
        _ => panic!("Expected DisconnectPeer"),
    }
}

#[test]
fn ipc_command_save_config_roundtrip() {
    let config = s_kvm_config::AppConfig::default();
    let cmd = IpcCommand::SaveConfig(config.clone());
    let json = serde_json::to_string(&cmd).unwrap();
    let decoded: IpcCommand = serde_json::from_str(&json).unwrap();
    match decoded {
        IpcCommand::SaveConfig(cfg) => {
            assert_eq!(cfg.machine_name, config.machine_name);
        }
        _ => panic!("Expected SaveConfig"),
    }
}

#[test]
fn ipc_command_update_screen_layout_roundtrip() {
    use s_kvm_core::{ScreenEdge, ScreenLink};

    let links = vec![ScreenLink {
        source_display: 0,
        source_edge: ScreenEdge::Right,
        target_peer: s_kvm_core::PeerId::new(),
        target_display: 0,
        offset: 0,
    }];
    let cmd = IpcCommand::UpdateScreenLayout(links.clone());
    let json = serde_json::to_string(&cmd).unwrap();
    let decoded: IpcCommand = serde_json::from_str(&json).unwrap();
    match decoded {
        IpcCommand::UpdateScreenLayout(decoded_links) => {
            assert_eq!(decoded_links.len(), 1);
            assert_eq!(decoded_links[0].source_display, 0);
        }
        _ => panic!("Expected UpdateScreenLayout"),
    }
}

// ── IPC response serialization roundtrips ────────────────────────────

#[test]
fn ipc_response_status_roundtrip() {
    let resp = IpcResponse::Status {
        active: true,
        connected_peers: 3,
        uptime_seconds: 120,
    };
    let json = serde_json::to_string(&resp).unwrap();
    let decoded: IpcResponse = serde_json::from_str(&json).unwrap();
    match decoded {
        IpcResponse::Status {
            active,
            connected_peers,
            uptime_seconds,
        } => {
            assert!(active);
            assert_eq!(connected_peers, 3);
            assert_eq!(uptime_seconds, 120);
        }
        _ => panic!("Expected Status"),
    }
}

#[test]
fn ipc_response_peers_roundtrip() {
    let resp = IpcResponse::Peers(vec![PeerStatusInfo {
        id: "peer-1".to_string(),
        hostname: "laptop".to_string(),
        os: "Windows".to_string(),
        state: ConnectionState::Active,
        latency_ms: Some(2.3),
    }]);
    let json = serde_json::to_string(&resp).unwrap();
    let decoded: IpcResponse = serde_json::from_str(&json).unwrap();
    match decoded {
        IpcResponse::Peers(peers) => {
            assert_eq!(peers.len(), 1);
            assert_eq!(peers[0].id, "peer-1");
            assert_eq!(peers[0].latency_ms, Some(2.3));
        }
        _ => panic!("Expected Peers"),
    }
}

#[test]
fn ipc_response_ok_roundtrip() {
    let resp = IpcResponse::Ok;
    let json = serde_json::to_string(&resp).unwrap();
    let decoded: IpcResponse = serde_json::from_str(&json).unwrap();
    assert!(matches!(decoded, IpcResponse::Ok));
}

#[test]
fn ipc_response_error_roundtrip() {
    let resp = IpcResponse::Error("something broke".to_string());
    let json = serde_json::to_string(&resp).unwrap();
    let decoded: IpcResponse = serde_json::from_str(&json).unwrap();
    match decoded {
        IpcResponse::Error(msg) => assert_eq!(msg, "something broke"),
        _ => panic!("Expected Error"),
    }
}
