import type { AppConfig, DisplayInfo, KvmStatus } from "../types";

const mockConfig: AppConfig = {
  peer_id: "mock-peer-id-1234",
  machine_name: "Dev Machine",
  network: {
    listen_port: 24800,
    mdns_enabled: true,
    mdns_service_type: "_softkvm._tcp.local.",
    static_peers: [],
  },
  screen_links: [],
  input: { raw_mouse_deltas: true, edge_switch_delay_ms: 50, edge_dead_zone: 2 },
  video: { enabled: false, max_fps: 60, target_bitrate_kbps: 20000, codec: "H264" },
  audio: { enabled: false, bitrate_kbps: 128, frame_size_ms: 10 },
  security: { cert_path: null, key_path: null, trusted_fingerprints: [], require_pairing: true },
  hotkeys: {
    toggle_active: "Ctrl+Alt+K",
    switch_screen: ["Ctrl+Alt+1", "Ctrl+Alt+2", "Ctrl+Alt+3", "Ctrl+Alt+4"],
    lock_screen: "Ctrl+Alt+L",
  },
};

const mockDisplays: DisplayInfo[] = [
  {
    id: 0,
    name: "Primary Display",
    x: 0,
    y: 0,
    width: 1920,
    height: 1080,
    refresh_rate: 60,
    scale_factor: 1,
    is_primary: true,
  },
];

const mockKvmStatus: KvmStatus = {
  active: false,
  active_peer: null,
  connected_peers: 0,
  uptime_seconds: 0,
};

export async function mockInvoke(cmd: string, _args?: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "get_config":
      return mockConfig;
    case "save_config":
      return null;
    case "get_peers":
      return [];
    case "get_connection_status":
      return "disconnected";
    case "connect_peer":
      return null;
    case "disconnect_peer":
      return null;
    case "get_displays":
      return mockDisplays;
    case "update_screen_layout":
      return null;
    case "start_kvm":
      return null;
    case "stop_kvm":
      return null;
    case "get_kvm_status":
      return mockKvmStatus;
    default:
      console.warn(`Mock: unknown command "${cmd}"`);
      return null;
  }
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function safeInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(cmd, args);
  }
  return mockInvoke(cmd, args) as T;
}
