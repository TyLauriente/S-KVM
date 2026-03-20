import { useState } from "react";

interface Peer {
  id: string;
  hostname: string;
  os: string;
  status: "connected" | "connecting" | "disconnected";
  latency?: number;
  displays: number;
}

function PeerList() {
  const [peers] = useState<Peer[]>([]);
  const [showAddDialog, setShowAddDialog] = useState(false);
  const [newPeerAddress, setNewPeerAddress] = useState("");
  const [newPeerPort, setNewPeerPort] = useState("24800");

  const statusColors: Record<string, string> = {
    connected: "var(--success)",
    connecting: "var(--warning)",
    disconnected: "var(--text-muted)",
  };

  return (
    <div className="peer-list">
      <div className="section-header">
        <h2 className="section-title">Connected Peers</h2>
        <button className="btn btn-primary" onClick={() => setShowAddDialog(true)}>
          Add Peer
        </button>
      </div>

      {peers.length === 0 ? (
        <div className="empty-state card">
          <div className="empty-icon">⊕</div>
          <h3>No peers connected</h3>
          <p>Peers on your network will appear here automatically via mDNS discovery,
             or you can add them manually.</p>
          <button className="btn btn-primary" onClick={() => setShowAddDialog(true)}>
            Add Peer Manually
          </button>
        </div>
      ) : (
        <div className="peers-grid">
          {peers.map((peer) => (
            <div key={peer.id} className="peer-card card">
              <div className="peer-header">
                <span
                  className="peer-status-dot"
                  style={{ background: statusColors[peer.status] }}
                />
                <h3 className="peer-hostname">{peer.hostname}</h3>
                <span className="peer-os">{peer.os}</span>
              </div>
              <div className="peer-details">
                <div className="peer-detail">
                  <span className="detail-label">Status</span>
                  <span className="detail-value">{peer.status}</span>
                </div>
                {peer.latency && (
                  <div className="peer-detail">
                    <span className="detail-label">Latency</span>
                    <span className="detail-value">{peer.latency}ms</span>
                  </div>
                )}
                <div className="peer-detail">
                  <span className="detail-label">Displays</span>
                  <span className="detail-value">{peer.displays}</span>
                </div>
              </div>
              <div className="peer-actions">
                <button className="btn btn-secondary">Configure</button>
                <button className="btn btn-danger">Disconnect</button>
              </div>
            </div>
          ))}
        </div>
      )}

      {showAddDialog && (
        <div className="dialog-overlay" onClick={() => setShowAddDialog(false)}>
          <div className="dialog card" onClick={(e) => e.stopPropagation()}>
            <h3>Add Peer</h3>
            <div className="dialog-form">
              <label>Address</label>
              <input
                value={newPeerAddress}
                onChange={(e) => setNewPeerAddress(e.target.value)}
                placeholder="192.168.1.100 or hostname"
              />
              <label>Port</label>
              <input
                value={newPeerPort}
                onChange={(e) => setNewPeerPort(e.target.value)}
                placeholder="24800"
              />
            </div>
            <div className="dialog-actions">
              <button className="btn btn-secondary" onClick={() => setShowAddDialog(false)}>
                Cancel
              </button>
              <button className="btn btn-primary" onClick={() => {
                // TODO: Call connect_peer command
                setShowAddDialog(false);
              }}>
                Connect
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

        .empty-state {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          padding: 60px 40px;
          text-align: center;
          gap: 12px;
        }

        .empty-icon {
          font-size: 48px;
          color: var(--text-muted);
        }

        .empty-state h3 {
          color: var(--text-primary);
          font-size: 18px;
        }

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
        }

        .peer-header {
          display: flex;
          align-items: center;
          gap: 10px;
        }

        .peer-status-dot {
          width: 10px;
          height: 10px;
          border-radius: 50%;
          flex-shrink: 0;
        }

        .peer-hostname {
          font-size: 16px;
          font-weight: 600;
          flex: 1;
        }

        .peer-os {
          font-size: 12px;
          color: var(--text-muted);
          background: var(--bg-tertiary);
          padding: 2px 8px;
          border-radius: 4px;
        }

        .peer-details {
          display: grid;
          grid-template-columns: repeat(3, 1fr);
          gap: 8px;
        }

        .peer-detail {
          display: flex;
          flex-direction: column;
          gap: 2px;
        }

        .detail-label {
          font-size: 11px;
          color: var(--text-muted);
          text-transform: uppercase;
        }

        .detail-value {
          font-size: 14px;
          color: var(--text-primary);
        }

        .peer-actions {
          display: flex;
          gap: 8px;
          justify-content: flex-end;
        }

        .dialog-overlay {
          position: fixed;
          inset: 0;
          background: rgba(0, 0, 0, 0.6);
          display: flex;
          align-items: center;
          justify-content: center;
          z-index: 100;
        }

        .dialog {
          width: 400px;
          display: flex;
          flex-direction: column;
          gap: 16px;
        }

        .dialog h3 {
          font-size: 18px;
        }

        .dialog-form {
          display: grid;
          grid-template-columns: 80px 1fr;
          gap: 10px;
          align-items: center;
        }

        .dialog-form label {
          font-size: 13px;
          color: var(--text-secondary);
        }

        .dialog-actions {
          display: flex;
          gap: 8px;
          justify-content: flex-end;
        }
      `}</style>
    </div>
  );
}

export default PeerList;
