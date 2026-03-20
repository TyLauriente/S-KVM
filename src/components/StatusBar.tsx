interface StatusBarProps {
  kvmActive: boolean;
}

function StatusBar({ kvmActive }: StatusBarProps) {
  return (
    <div className="status-bar">
      <div className="status-left">
        <span className={`status-indicator ${kvmActive ? "active" : ""}`}>
          <span className="status-dot-sm" />
          {kvmActive ? "Active" : "Inactive"}
        </span>
        <span className="status-divider">|</span>
        <span className="status-text">0 peers connected</span>
      </div>
      <div className="status-right">
        <span className="status-text">Latency: --</span>
        <span className="status-divider">|</span>
        <span className="status-text">S-KVM v0.1.0</span>
      </div>

      <style>{`
        .status-bar {
          position: fixed;
          bottom: 0;
          left: var(--sidebar-width);
          right: 0;
          height: var(--statusbar-height);
          background: var(--bg-secondary);
          border-top: 1px solid var(--border);
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 0 16px;
          font-size: 12px;
          z-index: 50;
        }

        .status-left, .status-right {
          display: flex;
          align-items: center;
          gap: 8px;
        }

        .status-indicator {
          display: flex;
          align-items: center;
          gap: 6px;
          color: var(--text-muted);
        }

        .status-indicator.active {
          color: var(--success);
        }

        .status-dot-sm {
          width: 6px;
          height: 6px;
          border-radius: 50%;
          background: var(--text-muted);
        }

        .status-indicator.active .status-dot-sm {
          background: var(--success);
        }

        .status-divider {
          color: var(--border);
        }

        .status-text {
          color: var(--text-muted);
        }
      `}</style>
    </div>
  );
}

export default StatusBar;
