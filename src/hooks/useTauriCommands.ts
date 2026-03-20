import { useCallback, useEffect, useState } from "react";
import type {
  AppConfig,
  KvmStatus,
  PeerStatus,
  ScreenLayoutUpdate,
} from "../types";
import { isTauri, safeInvoke } from "../mocks/tauriMock";

// --- Config hooks ---

export function useConfig() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadConfig = useCallback(async () => {
    try {
      setLoading(true);
      const cfg = await safeInvoke<AppConfig>("get_config");
      setConfig(cfg);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const saveConfig = useCallback(async (newConfig: AppConfig) => {
    try {
      await safeInvoke("save_config", { config: newConfig });
      setConfig(newConfig);
      setError(null);
    } catch (e) {
      setError(String(e));
      throw e;
    }
  }, []);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  return { config, loading, error, saveConfig, reloadConfig: loadConfig };
}

// --- Peer hooks ---

export function usePeers() {
  const [peers, setPeers] = useState<PeerStatus[]>([]);
  const [loading, setLoading] = useState(true);

  const loadPeers = useCallback(async () => {
    try {
      const result = await safeInvoke<PeerStatus[]>("get_peers");
      setPeers(result);
    } catch (e) {
      console.error("Failed to load peers:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  const connectPeer = useCallback(async (address: string, port: number) => {
    await safeInvoke("connect_peer", { address, port });
    await loadPeers();
  }, [loadPeers]);

  const disconnectPeer = useCallback(async (peerId: string) => {
    await safeInvoke("disconnect_peer", { peerId });
    await loadPeers();
  }, [loadPeers]);

  useEffect(() => {
    loadPeers();
    const interval = setInterval(loadPeers, 2000);
    return () => clearInterval(interval);
  }, [loadPeers]);

  return { peers, loading, connectPeer, disconnectPeer, reloadPeers: loadPeers };
}

// --- KVM Status hooks ---

export function useKvmStatus() {
  const [status, setStatus] = useState<KvmStatus>({
    active: false,
    active_peer: null,
    connected_peers: 0,
    uptime_seconds: 0,
  });

  const loadStatus = useCallback(async () => {
    try {
      const result = await safeInvoke<KvmStatus>("get_kvm_status");
      setStatus(result);
    } catch (e) {
      console.error("Failed to load KVM status:", e);
    }
  }, []);

  const startKvm = useCallback(async () => {
    await safeInvoke("start_kvm");
    await loadStatus();
  }, [loadStatus]);

  const stopKvm = useCallback(async () => {
    await safeInvoke("stop_kvm");
    await loadStatus();
  }, [loadStatus]);

  const toggleKvm = useCallback(async () => {
    if (status.active) {
      await stopKvm();
    } else {
      await startKvm();
    }
  }, [status.active, startKvm, stopKvm]);

  // Listen for backend events
  useEffect(() => {
    let unlisten: (() => void) | undefined;

    if (isTauri()) {
      import("@tauri-apps/api/event").then(({ listen }) => {
        listen<boolean>("kvm-status-changed", (event) => {
          setStatus((prev) => ({ ...prev, active: event.payload }));
        }).then((fn) => {
          unlisten = fn;
        });
      });
    }

    // Poll status periodically
    loadStatus();
    const interval = setInterval(loadStatus, 3000);

    return () => {
      unlisten?.();
      clearInterval(interval);
    };
  }, [loadStatus]);

  return { status, startKvm, stopKvm, toggleKvm };
}

// --- Screen Layout hooks ---

export function useScreenLayout() {
  const updateLayout = useCallback(async (layout: ScreenLayoutUpdate) => {
    await safeInvoke("update_screen_layout", { layout });
  }, []);

  return { updateLayout };
}
