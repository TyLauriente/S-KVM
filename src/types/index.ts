// TypeScript interfaces matching Rust types from crates/core/src/types.rs
// and crates/config/src/settings.rs

export type PeerId = string;
export type OsType = "Linux" | "Windows" | "MacOS";
export type ConnectionState =
  | "Disconnected"
  | "Connecting"
  | "Connected"
  | "Authenticated"
  | "Active";
export type ScreenEdge = "Top" | "Bottom" | "Left" | "Right";
export type VideoCodec = "H264" | "H265" | "VP9" | "AV1";

export interface DisplayInfo {
  id: number;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  refresh_rate: number;
  scale_factor: number;
  is_primary: boolean;
}

export interface PeerCapabilities {
  input_forwarding: boolean;
  display_streaming: boolean;
  audio_sharing: boolean;
  clipboard_sharing: boolean;
  fido2_forwarding: boolean;
}

export interface PeerInfo {
  id: PeerId;
  hostname: string;
  os: OsType;
  displays: DisplayInfo[];
  capabilities: PeerCapabilities;
}

export interface ScreenLink {
  source_display: number;
  source_edge: ScreenEdge;
  target_peer: PeerId;
  target_display: number;
  offset: number;
}

// Matches Rust PeerStatus from src-tauri/src/commands.rs
export interface PeerStatus {
  info: PeerInfo;
  state: ConnectionState;
  latency_ms: number | null;
}

export interface KvmStatus {
  active: boolean;
  active_peer: string | null;
  connected_peers: number;
  uptime_seconds: number;
}

export interface StaticPeer {
  address: string;
  port: number;
  name: string | null;
}

export interface NetworkConfig {
  listen_port: number;
  mdns_enabled: boolean;
  mdns_service_type: string;
  static_peers: StaticPeer[];
}

export interface InputConfig {
  raw_mouse_deltas: boolean;
  edge_switch_delay_ms: number;
  edge_dead_zone: number;
}

export interface VideoConfig {
  enabled: boolean;
  max_fps: number;
  target_bitrate_kbps: number;
  codec: VideoCodec;
}

export interface AudioConfig {
  enabled: boolean;
  bitrate_kbps: number;
  frame_size_ms: number;
}

export interface TrustedFingerprint {
  peer_id: PeerId;
  fingerprint: string;
  first_seen: string;
  hostname: string;
}

export interface SecurityConfig {
  cert_path: string | null;
  key_path: string | null;
  trusted_fingerprints: TrustedFingerprint[];
  require_pairing: boolean;
}

export interface HotkeyConfig {
  toggle_active: string;
  switch_screen: string[];
  lock_screen: string;
}

export interface AppConfig {
  peer_id: PeerId;
  machine_name: string;
  network: NetworkConfig;
  screen_links: ScreenLink[];
  input: InputConfig;
  video: VideoConfig;
  audio: AudioConfig;
  security: SecurityConfig;
  hotkeys: HotkeyConfig;
}

export interface ScreenLayoutUpdate {
  links: ScreenLink[];
}
