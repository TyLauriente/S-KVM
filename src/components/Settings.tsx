import { useState } from "react";

type SettingsTab = "general" | "network" | "input" | "video" | "audio" | "security" | "scripting";

function Settings() {
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");

  const tabs: { id: SettingsTab; label: string }[] = [
    { id: "general", label: "General" },
    { id: "network", label: "Network" },
    { id: "input", label: "Input" },
    { id: "video", label: "Video" },
    { id: "audio", label: "Audio" },
    { id: "security", label: "Security" },
    { id: "scripting", label: "Scripting" },
  ];

  return (
    <div className="settings">
      <div className="section-header">
        <h2 className="section-title">Settings</h2>
        <button className="btn btn-primary">Save Changes</button>
      </div>

      <div className="settings-layout">
        <div className="settings-tabs">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              className={`settings-tab ${activeTab === tab.id ? "active" : ""}`}
              onClick={() => setActiveTab(tab.id)}
            >
              {tab.label}
            </button>
          ))}
        </div>

        <div className="settings-content card">
          {activeTab === "general" && <GeneralSettings />}
          {activeTab === "network" && <NetworkSettings />}
          {activeTab === "input" && <InputSettings />}
          {activeTab === "video" && <VideoSettings />}
          {activeTab === "audio" && <AudioSettings />}
          {activeTab === "security" && <SecuritySettings />}
          {activeTab === "scripting" && <ScriptingSettings />}
        </div>
      </div>

      <style>{`
        .settings {
          height: 100%;
          display: flex;
          flex-direction: column;
        }

        .settings-layout {
          display: flex;
          gap: 16px;
          flex: 1;
          overflow: hidden;
        }

        .settings-tabs {
          display: flex;
          flex-direction: column;
          gap: 2px;
          min-width: 140px;
        }

        .settings-tab {
          padding: 8px 12px;
          background: transparent;
          color: var(--text-secondary);
          text-align: left;
          border-radius: var(--radius-sm);
          font-size: 14px;
        }

        .settings-tab:hover {
          background: var(--bg-hover);
          color: var(--text-primary);
        }

        .settings-tab.active {
          background: var(--accent-dim);
          color: white;
        }

        .settings-content {
          flex: 1;
          overflow-y: auto;
        }

        .settings-group {
          margin-bottom: 24px;
        }

        .settings-group h3 {
          font-size: 14px;
          color: var(--text-secondary);
          margin-bottom: 12px;
          text-transform: uppercase;
          letter-spacing: 0.5px;
        }

        .setting-row {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 10px 0;
          border-bottom: 1px solid var(--border);
        }

        .setting-row:last-child {
          border-bottom: none;
        }

        .setting-info {
          display: flex;
          flex-direction: column;
          gap: 2px;
        }

        .setting-label {
          font-size: 14px;
          color: var(--text-primary);
        }

        .setting-description {
          font-size: 12px;
          color: var(--text-muted);
        }

        .toggle {
          width: 44px;
          height: 24px;
          background: var(--bg-tertiary);
          border-radius: 12px;
          position: relative;
          cursor: pointer;
          transition: background 0.2s;
        }

        .toggle.active {
          background: var(--accent);
        }

        .toggle::after {
          content: '';
          position: absolute;
          top: 2px;
          left: 2px;
          width: 20px;
          height: 20px;
          border-radius: 50%;
          background: white;
          transition: transform 0.2s;
        }

        .toggle.active::after {
          transform: translateX(20px);
        }
      `}</style>
    </div>
  );
}

function GeneralSettings() {
  return (
    <div>
      <div className="settings-group">
        <h3>Machine</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Machine Name</span>
            <span className="setting-description">How this computer appears to other peers</span>
          </div>
          <input defaultValue="My Computer" style={{ width: 200 }} />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Start on Login</span>
            <span className="setting-description">Launch S-KVM when you log in</span>
          </div>
          <div className="toggle active" />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Start Minimized</span>
            <span className="setting-description">Start in system tray</span>
          </div>
          <div className="toggle" />
        </div>
      </div>
      <div className="settings-group">
        <h3>Hotkeys</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Toggle KVM</span>
            <span className="setting-description">Enable/disable input forwarding</span>
          </div>
          <input defaultValue="Ctrl+Alt+K" style={{ width: 150 }} />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Lock to Screen</span>
            <span className="setting-description">Prevent cursor from leaving current screen</span>
          </div>
          <input defaultValue="Ctrl+Alt+L" style={{ width: 150 }} />
        </div>
      </div>
    </div>
  );
}

function NetworkSettings() {
  return (
    <div>
      <div className="settings-group">
        <h3>Connection</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Listen Port</span>
            <span className="setting-description">QUIC port for incoming connections</span>
          </div>
          <input type="number" defaultValue="24800" style={{ width: 100 }} />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">mDNS Discovery</span>
            <span className="setting-description">Automatically discover peers on the network</span>
          </div>
          <div className="toggle active" />
        </div>
      </div>
    </div>
  );
}

function InputSettings() {
  return (
    <div>
      <div className="settings-group">
        <h3>Mouse</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Raw Mouse Deltas</span>
            <span className="setting-description">Forward raw mouse movement without acceleration</span>
          </div>
          <div className="toggle active" />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Edge Switch Delay</span>
            <span className="setting-description">Milliseconds before cursor transitions to another screen</span>
          </div>
          <input type="number" defaultValue="50" style={{ width: 80 }} /> <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>ms</span>
        </div>
      </div>
    </div>
  );
}

function VideoSettings() {
  return (
    <div>
      <div className="settings-group">
        <h3>Display Streaming</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Enable Display Streaming</span>
            <span className="setting-description">Stream display content to remote peers</span>
          </div>
          <div className="toggle" />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Max FPS</span>
          </div>
          <input type="number" defaultValue="60" style={{ width: 80 }} />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Codec</span>
          </div>
          <select defaultValue="h264">
            <option value="h264">H.264</option>
            <option value="h265">H.265</option>
            <option value="vp9">VP9</option>
            <option value="av1">AV1</option>
          </select>
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Target Bitrate</span>
          </div>
          <input type="number" defaultValue="20000" style={{ width: 100 }} /> <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>kbps</span>
        </div>
      </div>
    </div>
  );
}

function AudioSettings() {
  return (
    <div>
      <div className="settings-group">
        <h3>Audio Sharing</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Enable Audio Sharing</span>
            <span className="setting-description">Share audio between connected peers</span>
          </div>
          <div className="toggle" />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Opus Bitrate</span>
          </div>
          <input type="number" defaultValue="128" style={{ width: 80 }} /> <span style={{ color: 'var(--text-muted)', fontSize: 12 }}>kbps</span>
        </div>
      </div>
    </div>
  );
}

function SecuritySettings() {
  return (
    <div>
      <div className="settings-group">
        <h3>Authentication</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Require Pairing</span>
            <span className="setting-description">New peers must complete a pairing ceremony</span>
          </div>
          <div className="toggle active" />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Certificate Fingerprint</span>
            <span className="setting-description">SHA-256 fingerprint of this machine's certificate</span>
          </div>
          <code style={{ fontSize: 11, color: 'var(--text-muted)' }}>Generating...</code>
        </div>
      </div>
      <div className="settings-group">
        <h3>Trusted Peers</h3>
        <p style={{ color: 'var(--text-muted)', fontSize: 13 }}>No trusted peers yet. Connect to a peer to establish trust.</p>
      </div>
    </div>
  );
}

function ScriptingSettings() {
  return (
    <div>
      <div className="settings-group">
        <h3>Rhai Scripting</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Enable Scripting</span>
            <span className="setting-description">Allow Rhai scripts for automation</span>
          </div>
          <div className="toggle" />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Scripts Directory</span>
            <span className="setting-description">Location of user scripts</span>
          </div>
          <input defaultValue="~/.config/s-kvm/scripts" style={{ width: 250 }} />
        </div>
      </div>
    </div>
  );
}

export default Settings;
