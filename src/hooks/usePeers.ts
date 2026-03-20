import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PeerStatus } from "../types";
import { useTauriEvent } from "./useTauriEvents";

interface PeerEvent {
  peer_id: string;
  state: string;
  latency_ms: number | null;
}

export function usePeers() {
  const [peers, setPeers] = useState<PeerStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchPeers = useCallback(async () => {
    try {
      setLoading(true);
      const result = await invoke<PeerStatus[]>("get_peers");
      setPeers(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const connectPeer = useCallback(
    async (address: string, port: number): Promise<boolean> => {
      try {
        setError(null);
        await invoke("connect_peer", { address, port });
        await fetchPeers();
        return true;
      } catch (e) {
        setError(String(e));
        return false;
      }
    },
    [fetchPeers],
  );

  const disconnectPeer = useCallback(
    async (peerId: string): Promise<boolean> => {
      try {
        setError(null);
        await invoke("disconnect_peer", { peer_id: peerId });
        await fetchPeers();
        return true;
      } catch (e) {
        setError(String(e));
        return false;
      }
    },
    [fetchPeers],
  );

  const handlePeerEvent = useCallback(
    (event: PeerEvent) => {
      setPeers((prev) => {
        const idx = prev.findIndex((p) => p.id === event.peer_id);
        if (idx === -1) {
          fetchPeers();
          return prev;
        }
        return prev.map((p) =>
          p.id === event.peer_id
            ? {
                ...p,
                state: event.state as PeerStatus["state"],
                latency_ms: event.latency_ms,
              }
            : p,
        );
      });
    },
    [fetchPeers],
  );

  useTauriEvent("peer-status-changed", handlePeerEvent);

  useEffect(() => {
    fetchPeers();
  }, [fetchPeers]);

  return { peers, loading, error, connectPeer, disconnectPeer, refresh: fetchPeers };
}
