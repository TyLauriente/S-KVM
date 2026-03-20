import { useKvmStatus, usePeers } from "../hooks/useTauriCommands";

function Dashboard() {
  const { status, toggleKvm } = useKvmStatus();
  const { peers } = usePeers();

  const connectedPeers = peers.filter(
    (p) => p.state === "Connected" || p.state === "Active" || p.state === "Authenticated"
  );

  const formatUptime = (seconds: number): string => {
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    if (h > 0) return `${h}h ${m}m ${s}s`;
    if (m > 0) return `${m}m ${s}s`;
    return `${s}s`;
  };

  return (
    <div className="dashboard">
      <div className="section-header">
        <h2 className="section-title">Dashboard</h2>
      </div>

      {/* KVM Status Card */}
      <div className="dashboard-grid">
        <div className="status-card card">
          <div className="status-card-header">
            <h3>KVM Status</h3>
            <button
              className={`btn ${status.active ? "btn-danger" : "btn-success"}`}
              onClick={toggleKvm}
            >
              {status.active ? "Deactivate" : "Activate"}
            </button>
          </div>
          <div className="status-indicator-large">
            <div className={`status-ring ${status.active ? "active" : "inactive"}`}>
              <span className={`status-dot-large ${status.active ? "active" : "inactive"}`} />
            </div>
            <span className="status-label">{status.active ? "Active" : "Inactive"}</span>
          </div>
          <div className="status-details">
            <div className="stat">
              <span className="stat-value">{formatUptime(status.uptime_seconds)}</span>
              <span className="stat-label">Uptime</span>
            </div>
            <div className="stat">
              <span className="stat-value">{status.connected_peers}</span>
              <span className="stat-label">Peers</span>
            </div>
            <div className="stat">
              <span className="stat-value">{status.active_peer ?? "Local"}</span>
              <span className="stat-label">Active Screen</span>
            </div>
          </div>
        </div>

        {/* Quick Actions */}
        <div className="actions-card card">
          <h3>Quick Actions</h3>
          <div className="quick-actions">
            <button className="action-btn" onClick={toggleKvm}>
              <span className="action-icon">{status.active ? "⏸" : "▶"}</span>
              <span>{status.active ? "Pause KVM" : "Start KVM"}</span>
            </button>
            {connectedPeers.map((peer) => (
              <button key={peer.id} className="action-btn">
                <span className="action-icon">🖥</span>
                <span>Switch to {peer.hostname}</span>
              </button>
            ))}
            <button className="action-btn">
              <span className="action-icon">🔒</span>
              <span>Lock to Screen</span>
            </button>
            <button className="action-btn">
              <span className="action-icon">📋</span>
              <span>Sync Clipboard</span>
            </button>
          </div>
        </div>

        {/* Connected Peers */}
        <div className="peers-card card">
          <h3>Connected Peers</h3>
          {connectedPeers.length === 0 ? (
            <div className="empty-mini">
              <p>No peers connected. Peers will appear here when discovered via mDNS or connected manually.</p>
            </div>
          ) : (
            <div className="peer-list-compact">
              {connectedPeers.map((peer) => (
                <div key={peer.id} className="peer-row">
                  <span className="peer-dot active" />
                  <span className="peer-name">{peer.hostname}</span>
                  <span className="peer-os-badge">{peer.os}</span>
                  {peer.latency_ms && (
                    <span className="peer-latency">{peer.latency_ms.toFixed(1)}ms</span>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* System Info */}
        <div className="info-card card">
          <h3>System</h3>
          <div className="info-rows">
            <div className="info-row">
              <span className="info-label">Version</span>
              <span className="info-value">0.1.0</span>
            </div>
            <div className="info-row">
              <span className="info-label">Transport</span>
              <span className="info-value">QUIC (TLS 1.3)</span>
            </div>
            <div className="info-row">
              <span className="info-label">Discovery</span>
              <span className="info-value">mDNS-SD</span>
            </div>
            <div className="info-row">
              <span className="info-label">Input Backend</span>
              <span className="info-value">evdev/uinput</span>
            </div>
            <div className="info-row">
              <span className="info-label">Audio Codec</span>
              <span className="info-value">Opus</span>
            </div>
          </div>
        </div>
      </div>

      <style>{`
        .dashboard {
          height: 100%;
          overflow-y: auto;
        }

        .dashboard-grid {
          display: grid;
          grid-template-columns: repeat(2, 1fr);
          gap: 16px;
        }

        .status-card, .actions-card, .peers-card, .info-card {
          display: flex;
          flex-direction: column;
          gap: 16px;
        }

        .status-card h3, .actions-card h3, .peers-card h3, .info-card h3 {
          font-size: 14px;
          color: var(--text-secondary);
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .status-card-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
        }

        .status-indicator-large {
          display: flex;
          align-items: center;
          gap: 16px;
          padding: 12px 0;
        }

        .status-ring {
          width: 48px;
          height: 48px;
          border-radius: 50%;
          display: flex;
          align-items: center;
          justify-content: center;
          transition: all 0.3s ease;
        }

        .status-ring.active {
          background: rgba(34, 197, 94, 0.1);
          box-shadow: 0 0 20px rgba(34, 197, 94, 0.2);
        }

        .status-ring.inactive {
          background: rgba(102, 102, 119, 0.1);
        }

        .status-dot-large {
          width: 20px;
          height: 20px;
          border-radius: 50%;
        }

        .status-dot-large.active {
          background: var(--success);
          box-shadow: 0 0 12px rgba(34, 197, 94, 0.6);
          animation: pulse 2s ease-in-out infinite;
        }

        .status-dot-large.inactive {
          background: var(--text-muted);
        }

        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.6; }
        }

        .status-label {
          font-size: 24px;
          font-weight: 600;
        }

        .status-details {
          display: flex;
          gap: 24px;
        }

        .stat {
          display: flex;
          flex-direction: column;
          gap: 2px;
        }

        .stat-value {
          font-size: 18px;
          font-weight: 600;
          color: var(--text-primary);
        }

        .stat-label {
          font-size: 11px;
          color: var(--text-muted);
          text-transform: uppercase;
        }

        .quick-actions {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 8px;
        }

        .action-btn {
          display: flex;
          align-items: center;
          gap: 8px;
          padding: 10px 12px;
          background: var(--bg-tertiary);
          border-radius: var(--radius-sm);
          color: var(--text-primary);
          font-size: 13px;
          text-align: left;
        }

        .action-btn:hover {
          background: var(--bg-hover);
        }

        .action-icon {
          font-size: 16px;
        }

        .empty-mini {
          padding: 20px;
          text-align: center;
        }

        .empty-mini p {
          color: var(--text-muted);
          font-size: 13px;
          line-height: 1.5;
        }

        .peer-list-compact {
          display: flex;
          flex-direction: column;
          gap: 8px;
        }

        .peer-row {
          display: flex;
          align-items: center;
          gap: 10px;
          padding: 8px;
          background: var(--bg-tertiary);
          border-radius: var(--radius-sm);
        }

        .peer-dot {
          width: 8px;
          height: 8px;
          border-radius: 50%;
          flex-shrink: 0;
        }

        .peer-dot.active {
          background: var(--success);
        }

        .peer-name {
          flex: 1;
          font-size: 14px;
          font-weight: 500;
        }

        .peer-os-badge {
          font-size: 11px;
          color: var(--text-muted);
          background: var(--bg-primary);
          padding: 2px 6px;
          border-radius: 3px;
        }

        .peer-latency {
          font-size: 12px;
          color: var(--text-secondary);
        }

        .info-rows {
          display: flex;
          flex-direction: column;
          gap: 4px;
        }

        .info-row {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 6px 0;
          border-bottom: 1px solid var(--border);
        }

        .info-row:last-child {
          border-bottom: none;
        }

        .info-label {
          font-size: 13px;
          color: var(--text-muted);
        }

        .info-value {
          font-size: 13px;
          color: var(--text-primary);
        }
      `}</style>
    </div>
  );
}

export default Dashboard;
