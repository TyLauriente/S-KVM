import { useState, useCallback } from "react";
import { usePeers } from "../hooks/useTauriCommands";
import type { PeerStatus } from "../types";

function PeerList() {
  const { peers, connectPeer, disconnectPeer } = usePeers();
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [showPairingDialog, setShowPairingDialog] = useState(false);
  const [newPeerAddress, setNewPeerAddress] = useState("");
  const [newPeerPort, setNewPeerPort] = useState("24800");
  const [pairingCode, setPairingCode] = useState("");
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const statusColors: Record<string, string> = {
    Connected: "var(--success)",
    Active: "var(--success)",
    Authenticated: "var(--success)",
    Connecting: "var(--warning)",
    Disconnected: "var(--text-muted)",
  };

  const handleConnect = useCallback(async () => {
    if (!newPeerAddress.trim()) return;
    setConnecting(true);
    setError(null);
    try {
      await connectPeer(newPeerAddress.trim(), parseInt(newPeerPort) || 24800);
      setShowAddDialog(false);
      setNewPeerAddress("");
      setNewPeerPort("24800");
    } catch (e) {
      setError(String(e));
    } finally {
      setConnecting(false);
    }
  }, [newPeerAddress, newPeerPort, connectPeer]);

  const handleDisconnect = useCallback(async (peerId: string) => {
    try {
      await disconnectPeer(peerId);
    } catch (e) {
      setError(String(e));
    }
  }, [disconnectPeer]);

  return (
    <div className="peer-list">
      <div className="section-header">
        <h2 className="section-title">Connected Peers</h2>
        <div style={{ display: "flex", gap: "8px" }}>
          <button
            className="btn btn-secondary"
            onClick={() => setShowPairingDialog(true)}
          >
            Pairing Mode
          </button>
          <button
            className="btn btn-primary"
            onClick={() => setShowAddDialog(true)}
          >
            Add Peer
          </button>
        </div>
      </div>

      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>Dismiss</button>
        </div>
      )}

      {peers.length === 0 ? (
        <div className="empty-state card">
          <div className="empty-icon">⊕</div>
          <h3>No peers connected</h3>
          <p>
            Peers on your network will appear here automatically via mDNS
            discovery, or you can add them manually.
          </p>
          <div style={{ display: "flex", gap: "8px" }}>
            <button
              className="btn btn-secondary"
              onClick={() => setShowPairingDialog(true)}
            >
              Enter Pairing Code
            </button>
            <button
              className="btn btn-primary"
              onClick={() => setShowAddDialog(true)}
            >
              Add Peer Manually
            </button>
          </div>
        </div>
      ) : (
        <div className="peers-grid">
          {peers.map((peer: PeerStatus) => (
            <div key={peer.id} className="peer-card card">
              <div className="peer-header">
                <span
                  className="peer-status-dot"
                  style={{
                    background: statusColors[peer.state] || "var(--text-muted)",
                  }}
                />
                <h3 className="peer-hostname">{peer.hostname}</h3>
                <span className="peer-os">{peer.os}</span>
              </div>
              <div className="peer-details">
                <div className="peer-detail">
                  <span className="detail-label">Status</span>
                  <span className="detail-value">{peer.state}</span>
                </div>
                {peer.latency_ms !== null && (
                  <div className="peer-detail">
                    <span className="detail-label">Latency</span>
                    <span className="detail-value">
                      {peer.latency_ms?.toFixed(1)}ms
                    </span>
                  </div>
                )}
                <div className="peer-detail">
                  <span className="detail-label">Displays</span>
                  <span className="detail-value">{peer.displays.length}</span>
                </div>
              </div>
              <div className="peer-actions">
                <button className="btn btn-secondary">Configure</button>
                <button
                  className="btn btn-danger"
                  onClick={() => handleDisconnect(peer.id)}
                >
                  Disconnect
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Add Peer Dialog */}
      {showAddDialog && (
        <div
          className="dialog-overlay"
          onClick={() => setShowAddDialog(false)}
        >
          <div className="dialog card" onClick={(e) => e.stopPropagation()}>
            <h3>Add Peer</h3>
            <div className="dialog-form">
              <label>Address</label>
              <input
                value={newPeerAddress}
                onChange={(e) => setNewPeerAddress(e.target.value)}
                placeholder="192.168.1.100 or hostname"
                onKeyDown={(e) => e.key === "Enter" && handleConnect()}
                autoFocus
              />
              <label>Port</label>
              <input
                value={newPeerPort}
                onChange={(e) => setNewPeerPort(e.target.value)}
                placeholder="24800"
                type="number"
              />
            </div>
            {error && (
              <p style={{ color: "var(--danger)", fontSize: 13 }}>{error}</p>
            )}
            <div className="dialog-actions">
              <button
                className="btn btn-secondary"
                onClick={() => setShowAddDialog(false)}
              >
                Cancel
              </button>
              <button
                className="btn btn-primary"
                onClick={handleConnect}
                disabled={connecting || !newPeerAddress.trim()}
              >
                {connecting ? "Connecting..." : "Connect"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Pairing Dialog */}
      {showPairingDialog && (
        <div
          className="dialog-overlay"
          onClick={() => setShowPairingDialog(false)}
        >
          <div className="dialog card" onClick={(e) => e.stopPropagation()}>
            <h3>Peer Pairing</h3>
            <p style={{ color: "var(--text-secondary)", fontSize: 13, lineHeight: 1.5 }}>
              Enter the 6-digit pairing code displayed on the other machine,
              or share this machine's code with the other peer.
            </p>
            <div className="pairing-code-display">
              <span className="pairing-code">
                {Math.floor(100000 + Math.random() * 900000)}
              </span>
              <span style={{ fontSize: 12, color: "var(--text-muted)" }}>
                Your pairing code
              </span>
            </div>
            <div className="dialog-form">
              <label>Their Code</label>
              <input
                value={pairingCode}
                onChange={(e) => {
                  const val = e.target.value.replace(/\D/g, "").slice(0, 6);
                  setPairingCode(val);
                }}
                placeholder="000000"
                maxLength={6}
                style={{ textAlign: "center", fontSize: 24, letterSpacing: 8 }}
                autoFocus
              />
            </div>
            <div className="dialog-actions">
              <button
                className="btn btn-secondary"
                onClick={() => setShowPairingDialog(false)}
              >
                Cancel
              </button>
              <button
                className="btn btn-primary"
                disabled={pairingCode.length !== 6}
              >
                Pair
              </button>
            </div>
          </div>
        </div>
      )}

      <style>{`
        .peer-list {
          height: 100%;
          overflow-y: auto;
        }

        .error-banner {
          background: rgba(239, 68, 68, 0.1);
          border: 1px solid rgba(239, 68, 68, 0.3);
          border-radius: var(--radius-sm);
          padding: 8px 12px;
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 16px;
          color: var(--danger);
          font-size: 13px;
        }

        .error-banner button {
          background: none;
          color: var(--danger);
          font-size: 12px;
          text-decoration: underline;
        }

        .empty-state {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          padding: 60px 40px;
          text-align: center;
          gap: 12px;
        }

        .empty-icon { font-size: 48px; color: var(--text-muted); }

        .empty-state h3 { color: var(--text-primary); font-size: 18px; }

        .empty-state p {
          color: var(--text-secondary);
          font-size: 14px;
          max-width: 400px;
          line-height: 1.5;
          margin-bottom: 8px;
        }

        .peers-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
          gap: 16px;
        }

        .peer-card { display: flex; flex-direction: column; gap: 12px; }

        .peer-header { display: flex; align-items: center; gap: 10px; }

        .peer-status-dot {
          width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0;
        }

        .peer-hostname { font-size: 16px; font-weight: 600; flex: 1; }

        .peer-os {
          font-size: 12px; color: var(--text-muted);
          background: var(--bg-tertiary); padding: 2px 8px; border-radius: 4px;
        }

        .peer-details {
          display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px;
        }

        .peer-detail { display: flex; flex-direction: column; gap: 2px; }

        .detail-label {
          font-size: 11px; color: var(--text-muted); text-transform: uppercase;
        }

        .detail-value { font-size: 14px; color: var(--text-primary); }

        .peer-actions { display: flex; gap: 8px; justify-content: flex-end; }

        .dialog-overlay {
          position: fixed; inset: 0; background: rgba(0,0,0,0.6);
          display: flex; align-items: center; justify-content: center; z-index: 100;
        }

        .dialog {
          width: 420px; display: flex; flex-direction: column; gap: 16px;
        }

        .dialog h3 { font-size: 18px; }

        .dialog-form {
          display: grid; grid-template-columns: 80px 1fr;
          gap: 10px; align-items: center;
        }

        .dialog-form label { font-size: 13px; color: var(--text-secondary); }

        .dialog-actions { display: flex; gap: 8px; justify-content: flex-end; }

        .pairing-code-display {
          display: flex; flex-direction: column; align-items: center;
          gap: 8px; padding: 20px;
          background: var(--bg-tertiary); border-radius: var(--radius);
        }

        .pairing-code {
          font-size: 36px; font-weight: 700; letter-spacing: 8px;
          color: var(--accent); font-family: monospace;
        }
      `}</style>
    </div>
  );
}

export default PeerList;
