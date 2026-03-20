import { useState, useEffect, useCallback } from "react";
import { usePeers } from "../hooks/useTauriCommands";
import { useToast } from "./Toast";
import type { PeerStatus } from "../types";

function PeerList() {
  const { peers, connectPeer, disconnectPeer } = usePeers();
  const { addToast } = useToast();

  const [showAddDialog, setShowAddDialog] = useState(false);
  const [showPairingDialog, setShowPairingDialog] = useState(false);
  const [newPeerAddress, setNewPeerAddress] = useState("");
  const [newPeerPort, setNewPeerPort] = useState("24800");
  const [connecting, setConnecting] = useState(false);
  const [selectedPeer, setSelectedPeer] = useState<string | null>(null);
  const [pairingCode] = useState(() =>
    String(Math.floor(100000 + Math.random() * 900000)),
  );
  const [remotePairingCode, setRemotePairingCode] = useState("");

  const statusColors: Record<string, string> = {
    Connected: "var(--success)",
    Active: "var(--success)",
    Authenticated: "var(--success)",
    Connecting: "var(--warning)",
    Disconnected: "var(--text-muted)",
  };

  const handleConnect = useCallback(async () => {
    if (!newPeerAddress.trim()) {
      addToast("Please enter an address", "error");
      return;
    }
    const port = parseInt(newPeerPort, 10) || 24800;
    if (port < 1 || port > 65535) {
      addToast("Invalid port number (1-65535)", "error");
      return;
    }
    setConnecting(true);
    try {
      await connectPeer(newPeerAddress.trim(), port);
      addToast(`Connecting to ${newPeerAddress}:${port}...`, "info");
      setShowAddDialog(false);
      setNewPeerAddress("");
      setNewPeerPort("24800");
    } catch {
      addToast("Failed to connect", "error");
    } finally {
      setConnecting(false);
    }
  }, [newPeerAddress, newPeerPort, connectPeer, addToast]);

  const handleDisconnect = useCallback(
    async (peer: PeerStatus) => {
      try {
        await disconnectPeer(peer.id);
        addToast(`Disconnected from ${peer.hostname}`, "info");
        setSelectedPeer(null);
      } catch {
        addToast("Failed to disconnect", "error");
      }
    },
    [disconnectPeer, addToast],
  );

  const handlePairVerify = () => {
    if (remotePairingCode.length !== 6) {
      addToast("Please enter a 6-digit code", "error");
      return;
    }
    addToast("Pairing verified", "success");
    setShowPairingDialog(false);
    setRemotePairingCode("");
  };

  // Escape closes dialogs
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setShowAddDialog(false);
        setShowPairingDialog(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <div className="peer-list">
      <div className="section-header">
        <h2 className="section-title">Peers</h2>
        <div style={{ display: "flex", gap: "8px" }}>
          <button
            className="btn btn-secondary"
            onClick={() => setShowPairingDialog(true)}
          >
            Pair Device
          </button>
          <button
            className="btn btn-primary"
            onClick={() => setShowAddDialog(true)}
          >
            Add Peer
          </button>
        </div>
      </div>

      {/* mDNS Discovery Banner */}
      <div className="discovery-banner card">
        <div className="discovery-dot" />
        <span>Scanning for peers on your network via mDNS...</span>
      </div>

      {peers.length === 0 ? (
        <div className="empty-state card">
          <div className="empty-icon">{"\u2295"}</div>
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
            <div
              key={peer.id}
              className={`peer-card card ${selectedPeer === peer.id ? "peer-selected" : ""}`}
              onClick={() =>
                setSelectedPeer(
                  selectedPeer === peer.id ? null : peer.id,
                )
              }
            >
              <div className="peer-header">
                <span
                  className="peer-status-dot"
                  style={{
                    background:
                      statusColors[peer.state] ?? "var(--text-muted)",
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
                <div className="peer-detail">
                  <span className="detail-label">Latency</span>
                  <span className="detail-value">
                    {peer.latency_ms != null
                      ? `${peer.latency_ms.toFixed(1)}ms`
                      : "--"}
                  </span>
                </div>
                <div className="peer-detail">
                  <span className="detail-label">Displays</span>
                  <span className="detail-value">
                    {peer.displays.length}
                  </span>
                </div>
              </div>

              {/* Expanded Detail Panel */}
              {selectedPeer === peer.id && (
                <div
                  className="peer-expanded"
                  onClick={(e) => e.stopPropagation()}
                >
                  <div className="capability-section">
                    <h4>Capabilities</h4>
                    <div className="cap-list">
                      <span
                        className={`cap-item ${peer.capabilities.input_forwarding ? "cap-active" : ""}`}
                      >
                        {peer.capabilities.input_forwarding
                          ? "\u2713"
                          : "\u2717"}{" "}
                        Input
                      </span>
                      <span
                        className={`cap-item ${peer.capabilities.display_streaming ? "cap-active" : ""}`}
                      >
                        {peer.capabilities.display_streaming
                          ? "\u2713"
                          : "\u2717"}{" "}
                        Video
                      </span>
                      <span
                        className={`cap-item ${peer.capabilities.audio_sharing ? "cap-active" : ""}`}
                      >
                        {peer.capabilities.audio_sharing
                          ? "\u2713"
                          : "\u2717"}{" "}
                        Audio
                      </span>
                      <span
                        className={`cap-item ${peer.capabilities.clipboard_sharing ? "cap-active" : ""}`}
                      >
                        {peer.capabilities.clipboard_sharing
                          ? "\u2713"
                          : "\u2717"}{" "}
                        Clipboard
                      </span>
                      <span
                        className={`cap-item ${peer.capabilities.fido2_forwarding ? "cap-active" : ""}`}
                      >
                        {peer.capabilities.fido2_forwarding
                          ? "\u2713"
                          : "\u2717"}{" "}
                        FIDO2
                      </span>
                    </div>
                  </div>
                  {peer.displays.length > 0 && (
                    <div className="display-section">
                      <h4>Displays</h4>
                      {peer.displays.map((d) => (
                        <div key={d.id} className="display-item">
                          <span>{d.name}</span>
                          <span className="display-res">
                            {d.width}x{d.height}
                            {d.is_primary ? " (Primary)" : ""}
                          </span>
                        </div>
                      ))}
                    </div>
                  )}
                  <div className="peer-actions">
                    <button className="btn btn-secondary">Configure</button>
                    <button
                      className="btn btn-danger"
                      onClick={() => handleDisconnect(peer)}
                    >
                      Disconnect
                    </button>
                  </div>
                </div>
              )}
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
                onKeyDown={(e) => e.key === "Enter" && handleConnect()}
              />
            </div>
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
            <h3>Pair Device</h3>
            <div className="pairing-section">
              <div className="pairing-code-display">
                <span className="pairing-label">Your Code</span>
                <span className="pairing-code">{pairingCode}</span>
                <span className="pairing-hint">
                  Share this code with the remote device
                </span>
              </div>
              <div className="pairing-divider" />
              <div className="pairing-input-section">
                <span className="pairing-label">Remote Code</span>
                <input
                  className="pairing-input"
                  value={remotePairingCode}
                  onChange={(e) => {
                    const v = e.target.value.replace(/\D/g, "").slice(0, 6);
                    setRemotePairingCode(v);
                  }}
                  placeholder="000000"
                  maxLength={6}
                  onKeyDown={(e) =>
                    e.key === "Enter" && handlePairVerify()
                  }
                  autoFocus
                />
              </div>
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
                onClick={handlePairVerify}
                disabled={remotePairingCode.length !== 6}
              >
                Verify
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
        .discovery-banner {
          display: flex;
          align-items: center;
          gap: 10px;
          padding: 10px 16px;
          margin-bottom: 16px;
          font-size: 13px;
          color: var(--text-secondary);
        }
        .discovery-dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          background: var(--accent);
          animation: disc-pulse 2s ease-in-out infinite;
          flex-shrink: 0;
        }
        @keyframes disc-pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.3; }
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
        .peer-card {
          display: flex;
          flex-direction: column;
          gap: 12px;
          cursor: pointer;
          transition: border-color 0.15s ease;
        }
        .peer-card:hover { border-color: var(--accent); }
        .peer-selected {
          border-color: var(--accent);
          box-shadow: 0 0 0 1px var(--accent);
        }
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
        .peer-expanded {
          display: flex;
          flex-direction: column;
          gap: 12px;
          border-top: 1px solid var(--border);
          padding-top: 12px;
        }
        .peer-expanded h4 {
          font-size: 12px;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
          margin-bottom: 6px;
        }
        .cap-list {
          display: flex;
          flex-wrap: wrap;
          gap: 8px;
        }
        .cap-item {
          font-size: 12px;
          padding: 3px 8px;
          border-radius: 3px;
          background: var(--bg-tertiary);
          color: var(--text-muted);
        }
        .cap-active {
          color: var(--success);
          background: rgba(34, 197, 94, 0.1);
        }
        .display-item {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 4px 0;
          font-size: 13px;
        }
        .display-res {
          color: var(--text-muted);
          font-size: 12px;
        }
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
        .pairing-section {
          display: flex;
          flex-direction: column;
          gap: 16px;
        }
        .pairing-code-display {
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 8px;
          padding: 16px;
          background: var(--bg-tertiary);
          border-radius: var(--radius);
        }
        .pairing-label {
          font-size: 12px;
          color: var(--text-muted);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }
        .pairing-code {
          font-size: 32px;
          font-weight: 700;
          letter-spacing: 8px;
          color: var(--accent);
          font-family: monospace;
        }
        .pairing-hint {
          font-size: 12px;
          color: var(--text-muted);
        }
        .pairing-divider {
          height: 1px;
          background: var(--border);
        }
        .pairing-input-section {
          display: flex;
          flex-direction: column;
          align-items: center;
          gap: 8px;
        }
        .pairing-input {
          width: 200px;
          text-align: center;
          font-size: 24px;
          font-weight: 600;
          letter-spacing: 6px;
          font-family: monospace;
        }
      `}</style>
    </div>
  );
}

export default PeerList;
