//! mDNS-SD service discovery for finding S-KVM peers on the local network.

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use s_kvm_core::{OsType, PeerId};
use std::collections::HashMap;
use tokio::sync::mpsc;

const SERVICE_TYPE: &str = "_softkvm._tcp.local.";

/// Events emitted by the discovery service.
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A new S-KVM peer was discovered.
    PeerDiscovered {
        peer_id: PeerId,
        hostname: String,
        address: String,
        port: u16,
        os: OsType,
        version: String,
    },
    /// A previously discovered peer is no longer available.
    PeerLost {
        hostname: String,
    },
}

/// mDNS service discovery for S-KVM peers.
pub struct DiscoveryService {
    daemon: ServiceDaemon,
    peer_id: PeerId,
    hostname: String,
    port: u16,
}

impl DiscoveryService {
    /// Create a new discovery service.
    pub fn new(peer_id: PeerId, hostname: String, port: u16) -> Result<Self, DiscoveryError> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| DiscoveryError::Init(e.to_string()))?;

        Ok(Self {
            daemon,
            peer_id,
            hostname,
            port,
        })
    }

    /// Register this machine as an S-KVM service on the network.
    pub fn advertise(&self) -> Result<(), DiscoveryError> {
        let os_str = if cfg!(target_os = "linux") {
            "Linux"
        } else if cfg!(target_os = "windows") {
            "Windows"
        } else {
            "Unknown"
        };

        let mut properties = HashMap::new();
        properties.insert("version".to_string(), "0.1.0".to_string());
        properties.insert("peer_id".to_string(), self.peer_id.to_string());
        properties.insert("os".to_string(), os_str.to_string());

        let service_name = format!("S-KVM {}", self.hostname);
        let service_info = ServiceInfo::new(
            SERVICE_TYPE,
            &service_name,
            &format!("{}.", self.hostname),
            "",
            self.port,
            properties,
        )
        .map_err(|e| DiscoveryError::Registration(e.to_string()))?;

        self.daemon
            .register(service_info)
            .map_err(|e| DiscoveryError::Registration(e.to_string()))?;

        tracing::info!(
            hostname = %self.hostname,
            port = self.port,
            "Registered mDNS service"
        );

        Ok(())
    }

    /// Start browsing for other S-KVM peers.
    /// Returns a channel that receives discovery events.
    pub fn browse(&self) -> Result<mpsc::Receiver<DiscoveryEvent>, DiscoveryError> {
        let receiver = self
            .daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| DiscoveryError::Browse(e.to_string()))?;

        let (tx, rx) = mpsc::channel(32);
        let own_hostname = self.hostname.clone();

        std::thread::Builder::new()
            .name("mdns-browser".into())
            .spawn(move || {
                while let Ok(event) = receiver.recv() {
                    match event {
                        ServiceEvent::ServiceResolved(info) => {
                            let hostname = info.get_hostname().trim_end_matches('.').to_string();

                            // Skip our own service
                            if hostname == own_hostname {
                                continue;
                            }

                            let properties = info.get_properties();
                            let peer_id_str = properties
                                .get_property_val_str("peer_id")
                                .unwrap_or("");
                            let version = properties
                                .get_property_val_str("version")
                                .unwrap_or("unknown")
                                .to_string();
                            let os_str = properties
                                .get_property_val_str("os")
                                .unwrap_or("Unknown");

                            let os = match os_str {
                                "Linux" => OsType::Linux,
                                "Windows" => OsType::Windows,
                                "MacOS" => OsType::MacOS,
                                _ => OsType::Linux,
                            };

                            let peer_id = peer_id_str
                                .parse::<uuid::Uuid>()
                                .map(PeerId)
                                .unwrap_or_else(|_| PeerId::new());

                            let address = info
                                .get_addresses()
                                .iter()
                                .next()
                                .map(|a| a.to_string())
                                .unwrap_or_default();

                            tracing::info!(
                                hostname = %hostname,
                                address = %address,
                                port = info.get_port(),
                                "Discovered S-KVM peer"
                            );

                            let _ = tx.blocking_send(DiscoveryEvent::PeerDiscovered {
                                peer_id,
                                hostname,
                                address,
                                port: info.get_port(),
                                os,
                                version,
                            });
                        }
                        ServiceEvent::ServiceRemoved(_, fullname) => {
                            let hostname = fullname
                                .split('.')
                                .next()
                                .unwrap_or(&fullname)
                                .to_string();

                            tracing::info!(hostname = %hostname, "S-KVM peer lost");

                            let _ = tx.blocking_send(DiscoveryEvent::PeerLost { hostname });
                        }
                        _ => {}
                    }
                }
            })
            .map_err(|e| DiscoveryError::Browse(e.to_string()))?;

        Ok(rx)
    }

    /// Stop the discovery service.
    pub fn shutdown(self) -> Result<(), DiscoveryError> {
        self.daemon
            .shutdown()
            .map_err(|e| DiscoveryError::Shutdown(e.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("Failed to initialize mDNS: {0}")]
    Init(String),
    #[error("Failed to register service: {0}")]
    Registration(String),
    #[error("Failed to browse: {0}")]
    Browse(String),
    #[error("Failed to shutdown: {0}")]
    Shutdown(String),
}
