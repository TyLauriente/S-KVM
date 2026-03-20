// TypeScript interfaces matching Rust types from crates/core/src/

export interface PeerId {
  inner: string;
}

export interface PeerInfo {
  id: string;
  hostname: string;
  os: OsType;
  displays: DisplayInfo[];
  capabilities: PeerCapabilities;
}

export type OsType = "Linux" | "Windows" | "MacOS";

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

export type ScreenEdge = "Top" | "Bottom" | "Left" | "Right";

export interface ScreenLink {
  source_display: number;
  source_edge: ScreenEdge;
  target_peer: string;
  target_display: number;
  offset: number;
}

export type ConnectionState =
  | "Disconnected"
  | "Connecting"
  | "Connected"
  | "Authenticated"
  | "Active";

export interface PeerStatus {
  id: string;
  hostname: string;
  os: string;
  state: ConnectionState;
  latency_ms: number | null;
  displays: DisplayInfo[];
}

export interface KvmStatus {
  active: boolean;
  active_peer: string | null;
  connected_peers: number;
  uptime_seconds: number;
}

export interface AppConfig {
  peer_id: string;
  machine_name: string;
  network: NetworkConfig;
  screen_links: ScreenLink[];
  input: InputConfig;
  video: VideoConfig;
  audio: AudioConfig;
  security: SecurityConfig;
  hotkeys: HotkeyConfig;
}

export interface NetworkConfig {
  listen_port: number;
  mdns_enabled: boolean;
  mdns_service_type: string;
  static_peers: StaticPeer[];
}

export interface StaticPeer {
  address: string;
  port: number;
  name: string | null;
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

export type VideoCodec = "H264" | "H265" | "VP9" | "AV1";

export interface AudioConfig {
  enabled: boolean;
  bitrate_kbps: number;
  frame_size_ms: number;
}

export interface SecurityConfig {
  cert_path: string | null;
  key_path: string | null;
  trusted_fingerprints: TrustedFingerprint[];
  require_pairing: boolean;
}

export interface TrustedFingerprint {
  peer_id: string;
  fingerprint: string;
  first_seen: string;
  hostname: string;
}

export interface HotkeyConfig {
  toggle_active: string;
  switch_screen: string[];
  lock_screen: string;
}

export interface ScreenLayoutUpdate {
  links: ScreenLink[];
}
