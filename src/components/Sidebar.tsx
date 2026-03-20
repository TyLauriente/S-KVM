interface SidebarProps {
  currentView: string;
  onViewChange: (view: "dashboard" | "layout" | "peers" | "settings") => void;
  kvmActive: boolean;
  onToggleKvm: () => void;
}

function Sidebar({ currentView, onViewChange, kvmActive, onToggleKvm }: SidebarProps) {
  const navItems = [
    { id: "dashboard" as const, label: "Dashboard", icon: "◉" },
    { id: "layout" as const, label: "Monitor Layout", icon: "⊞" },
    { id: "peers" as const, label: "Peers", icon: "⊕" },
    { id: "settings" as const, label: "Settings", icon: "⚙" },
  ];

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h1 className="sidebar-logo">S-KVM</h1>
        <span className="sidebar-version">v0.1.0</span>
      </div>

      <nav className="sidebar-nav">
        {navItems.map((item) => (
          <button
            key={item.id}
            className={`sidebar-nav-item ${currentView === item.id ? "active" : ""}`}
            onClick={() => onViewChange(item.id)}
          >
            <span className="nav-icon">{item.icon}</span>
            <span className="nav-label">{item.label}</span>
          </button>
        ))}
      </nav>

      <div className="sidebar-footer">
        <button
          className={`kvm-toggle ${kvmActive ? "active" : ""}`}
          onClick={onToggleKvm}
        >
          <span className={`status-dot ${kvmActive ? "active" : "inactive"}`} />
          <span>{kvmActive ? "KVM Active" : "KVM Inactive"}</span>
        </button>
      </div>

      <style>{`
        .sidebar {
          width: var(--sidebar-width);
          background: var(--bg-secondary);
          border-right: 1px solid var(--border);
          display: flex;
          flex-direction: column;
          height: 100vh;
          flex-shrink: 0;
        }

        .sidebar-header {
          padding: 20px;
          border-bottom: 1px solid var(--border);
          display: flex;
          align-items: baseline;
          gap: 8px;
        }

        .sidebar-logo {
          font-size: 24px;
          font-weight: 700;
          color: var(--accent);
        }

        .sidebar-version {
          font-size: 12px;
          color: var(--text-muted);
        }

        .sidebar-nav {
          flex: 1;
          padding: 12px 8px;
          display: flex;
          flex-direction: column;
          gap: 2px;
        }

        .sidebar-nav-item {
          display: flex;
          align-items: center;
          gap: 12px;
          padding: 10px 12px;
          border-radius: var(--radius-sm);
          background: transparent;
          color: var(--text-secondary);
          width: 100%;
          text-align: left;
          font-size: 14px;
        }

        .sidebar-nav-item:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }

        .sidebar-nav-item.active {
          background: var(--accent-dim);
          color: white;
        }

        .nav-icon {
          font-size: 18px;
          width: 24px;
          text-align: center;
        }

        .sidebar-footer {
          padding: 12px;
          border-top: 1px solid var(--border);
        }

        .kvm-toggle {
          display: flex;
          align-items: center;
          gap: 10px;
          width: 100%;
          padding: 12px 16px;
          border-radius: var(--radius);
          background: var(--bg-tertiary);
          color: var(--text-primary);
          font-weight: 500;
        }

        .kvm-toggle:hover {
          background: var(--bg-hover);
        }

        .kvm-toggle.active {
          background: rgba(34, 197, 94, 0.15);
          border: 1px solid rgba(34, 197, 94, 0.3);
        }

        .status-dot {
          width: 10px;
          height: 10px;
          border-radius: 50%;
          flex-shrink: 0;
        }

        .status-dot.active {
          background: var(--success);
          box-shadow: 0 0 8px rgba(34, 197, 94, 0.5);
        }

        .status-dot.inactive {
          background: var(--text-muted);
        }
      `}</style>
    </aside>
  );
}

export default Sidebar;
