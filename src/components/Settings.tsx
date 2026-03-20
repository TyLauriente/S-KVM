import { useState, useEffect } from "react";
import { useConfig } from "../hooks/useTauriCommands";
import { useToast } from "./Toast";
import type { AppConfig, VideoCodec } from "../types";

type SettingsTab =
  | "general"
  | "network"
  | "input"
  | "video"
  | "audio"
  | "security"
  | "scripting";

type ConfigUpdater = (fn: (c: AppConfig) => AppConfig) => void;

function Settings() {
  const { config, loading, error, saveConfig } = useConfig();
  const { addToast } = useToast();
  const [activeTab, setActiveTab] = useState<SettingsTab>("general");
  const [localConfig, setLocalConfig] = useState<AppConfig | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (config && !localConfig) {
      setLocalConfig(config);
    }
  }, [config, localConfig]);

  const update: ConfigUpdater = (fn) => {
    setLocalConfig((prev) => (prev ? fn(prev) : prev));
  };

  const handleSave = async () => {
    if (!localConfig) return;
    setSaving(true);
    try {
      await saveConfig(localConfig);
      addToast("Settings saved", "success");
    } catch {
      addToast("Failed to save settings", "error");
    } finally {
      setSaving(false);
    }
  };

  const tabs: { id: SettingsTab; label: string }[] = [
    { id: "general", label: "General" },
    { id: "network", label: "Network" },
    { id: "input", label: "Input" },
    { id: "video", label: "Video" },
    { id: "audio", label: "Audio" },
    { id: "security", label: "Security" },
    { id: "scripting", label: "Scripting" },
  ];

  if (loading) {
    return (
      <div className="settings">
        <div className="section-header">
          <h2 className="section-title">Settings</h2>
        </div>
        <div className="card" style={{ padding: 40, textAlign: "center", color: "var(--text-muted)" }}>
          Loading configuration...
        </div>
      </div>
    );
  }

  if (error || !localConfig) {
    return (
      <div className="settings">
        <div className="section-header">
          <h2 className="section-title">Settings</h2>
        </div>
        <div className="card" style={{ padding: 40, textAlign: "center", color: "var(--danger)" }}>
          Failed to load configuration: {error ?? "Unknown error"}
        </div>
      </div>
    );
  }

  return (
    <div className="settings">
      <div className="section-header">
        <h2 className="section-title">Settings</h2>
        <button
          className="btn btn-primary"
          onClick={handleSave}
          disabled={saving}
        >
          {saving ? "Saving..." : "Save Changes"}
        </button>
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
          {activeTab === "general" && (
            <GeneralSettings config={localConfig} update={update} />
          )}
          {activeTab === "network" && (
            <NetworkSettings config={localConfig} update={update} />
          )}
          {activeTab === "input" && (
            <InputSettings config={localConfig} update={update} />
          )}
          {activeTab === "video" && (
            <VideoSettings config={localConfig} update={update} />
          )}
          {activeTab === "audio" && (
            <AudioSettings config={localConfig} update={update} />
          )}
          {activeTab === "security" && (
            <SecuritySettings config={localConfig} update={update} />
          )}
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
          flex-shrink: 0;
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
        .setting-input-row {
          display: flex;
          align-items: center;
          gap: 6px;
        }
        .setting-unit {
          color: var(--text-muted);
          font-size: 12px;
        }
        .input-error {
          border-color: var(--danger) !important;
        }
      `}</style>
    </div>
  );
}

interface SubSettingsProps {
  config: AppConfig;
  update: ConfigUpdater;
}

function GeneralSettings({ config, update }: SubSettingsProps) {
  const [startOnLogin, setStartOnLogin] = useState(true);
  const [startMinimized, setStartMinimized] = useState(false);

  return (
    <div>
      <div className="settings-group">
        <h3>Machine</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Machine Name</span>
            <span className="setting-description">
              How this computer appears to other peers
            </span>
          </div>
          <input
            value={config.machine_name}
            onChange={(e) =>
              update((c) => ({ ...c, machine_name: e.target.value }))
            }
            style={{ width: 200 }}
          />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Start on Login</span>
            <span className="setting-description">
              Launch S-KVM when you log in
            </span>
          </div>
          <div
            className={`toggle ${startOnLogin ? "active" : ""}`}
            onClick={() => setStartOnLogin(!startOnLogin)}
          />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Start Minimized</span>
            <span className="setting-description">Start in system tray</span>
          </div>
          <div
            className={`toggle ${startMinimized ? "active" : ""}`}
            onClick={() => setStartMinimized(!startMinimized)}
          />
        </div>
      </div>
      <div className="settings-group">
        <h3>Hotkeys</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Toggle KVM</span>
            <span className="setting-description">
              Enable/disable input forwarding
            </span>
          </div>
          <input
            value={config.hotkeys.toggle_active}
            onChange={(e) =>
              update((c) => ({
                ...c,
                hotkeys: { ...c.hotkeys, toggle_active: e.target.value },
              }))
            }
            style={{ width: 150 }}
          />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Lock to Screen</span>
            <span className="setting-description">
              Prevent cursor from leaving current screen
            </span>
          </div>
          <input
            value={config.hotkeys.lock_screen}
            onChange={(e) =>
              update((c) => ({
                ...c,
                hotkeys: { ...c.hotkeys, lock_screen: e.target.value },
              }))
            }
            style={{ width: 150 }}
          />
        </div>
      </div>
    </div>
  );
}

function NetworkSettings({ config, update }: SubSettingsProps) {
  const portValid =
    config.network.listen_port >= 1 && config.network.listen_port <= 65535;

  return (
    <div>
      <div className="settings-group">
        <h3>Connection</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Listen Port</span>
            <span className="setting-description">
              QUIC port for incoming connections
            </span>
          </div>
          <div className="setting-input-row">
            <input
              type="number"
              value={config.network.listen_port}
              onChange={(e) => {
                const val = parseInt(e.target.value, 10);
                if (!isNaN(val)) {
                  update((c) => ({
                    ...c,
                    network: { ...c.network, listen_port: val },
                  }));
                }
              }}
              className={portValid ? "" : "input-error"}
              style={{ width: 100 }}
            />
            {!portValid && (
              <span style={{ color: "var(--danger)", fontSize: 11 }}>
                1-65535
              </span>
            )}
          </div>
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">mDNS Discovery</span>
            <span className="setting-description">
              Automatically discover peers on the network
            </span>
          </div>
          <div
            className={`toggle ${config.network.mdns_enabled ? "active" : ""}`}
            onClick={() =>
              update((c) => ({
                ...c,
                network: {
                  ...c.network,
                  mdns_enabled: !c.network.mdns_enabled,
                },
              }))
            }
          />
        </div>
      </div>
    </div>
  );
}

function InputSettings({ config, update }: SubSettingsProps) {
  return (
    <div>
      <div className="settings-group">
        <h3>Mouse</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Raw Mouse Deltas</span>
            <span className="setting-description">
              Forward raw mouse movement without acceleration
            </span>
          </div>
          <div
            className={`toggle ${config.input.raw_mouse_deltas ? "active" : ""}`}
            onClick={() =>
              update((c) => ({
                ...c,
                input: {
                  ...c.input,
                  raw_mouse_deltas: !c.input.raw_mouse_deltas,
                },
              }))
            }
          />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Edge Switch Delay</span>
            <span className="setting-description">
              Milliseconds before cursor transitions to another screen
            </span>
          </div>
          <div className="setting-input-row">
            <input
              type="number"
              value={config.input.edge_switch_delay_ms}
              onChange={(e) => {
                const val = parseInt(e.target.value, 10);
                if (!isNaN(val) && val >= 0) {
                  update((c) => ({
                    ...c,
                    input: { ...c.input, edge_switch_delay_ms: val },
                  }));
                }
              }}
              style={{ width: 80 }}
            />
            <span className="setting-unit">ms</span>
          </div>
        </div>
      </div>
    </div>
  );
}

function VideoSettings({ config, update }: SubSettingsProps) {
  return (
    <div>
      <div className="settings-group">
        <h3>Display Streaming</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Enable Display Streaming</span>
            <span className="setting-description">
              Stream display content to remote peers
            </span>
          </div>
          <div
            className={`toggle ${config.video.enabled ? "active" : ""}`}
            onClick={() =>
              update((c) => ({
                ...c,
                video: { ...c.video, enabled: !c.video.enabled },
              }))
            }
          />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Max FPS</span>
          </div>
          <input
            type="number"
            value={config.video.max_fps}
            onChange={(e) => {
              const val = parseInt(e.target.value, 10);
              if (!isNaN(val) && val > 0) {
                update((c) => ({
                  ...c,
                  video: { ...c.video, max_fps: val },
                }));
              }
            }}
            style={{ width: 80 }}
          />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Codec</span>
          </div>
          <select
            value={config.video.codec}
            onChange={(e) =>
              update((c) => ({
                ...c,
                video: {
                  ...c.video,
                  codec: e.target.value as VideoCodec,
                },
              }))
            }
          >
            <option value="H264">H.264</option>
            <option value="H265">H.265</option>
            <option value="VP9">VP9</option>
            <option value="AV1">AV1</option>
          </select>
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Target Bitrate</span>
          </div>
          <div className="setting-input-row">
            <input
              type="number"
              value={config.video.target_bitrate_kbps}
              onChange={(e) => {
                const val = parseInt(e.target.value, 10);
                if (!isNaN(val) && val > 0) {
                  update((c) => ({
                    ...c,
                    video: { ...c.video, target_bitrate_kbps: val },
                  }));
                }
              }}
              style={{ width: 100 }}
            />
            <span className="setting-unit">kbps</span>
          </div>
        </div>
      </div>
    </div>
  );
}

function AudioSettings({ config, update }: SubSettingsProps) {
  return (
    <div>
      <div className="settings-group">
        <h3>Audio Sharing</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Enable Audio Sharing</span>
            <span className="setting-description">
              Share audio between connected peers
            </span>
          </div>
          <div
            className={`toggle ${config.audio.enabled ? "active" : ""}`}
            onClick={() =>
              update((c) => ({
                ...c,
                audio: { ...c.audio, enabled: !c.audio.enabled },
              }))
            }
          />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Opus Bitrate</span>
          </div>
          <div className="setting-input-row">
            <input
              type="number"
              value={config.audio.bitrate_kbps}
              onChange={(e) => {
                const val = parseInt(e.target.value, 10);
                if (!isNaN(val) && val > 0) {
                  update((c) => ({
                    ...c,
                    audio: { ...c.audio, bitrate_kbps: val },
                  }));
                }
              }}
              style={{ width: 80 }}
            />
            <span className="setting-unit">kbps</span>
          </div>
        </div>
      </div>
    </div>
  );
}

function SecuritySettings({ config, update }: SubSettingsProps) {
  return (
    <div>
      <div className="settings-group">
        <h3>Authentication</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Require Pairing</span>
            <span className="setting-description">
              New peers must complete a pairing ceremony
            </span>
          </div>
          <div
            className={`toggle ${config.security.require_pairing ? "active" : ""}`}
            onClick={() =>
              update((c) => ({
                ...c,
                security: {
                  ...c.security,
                  require_pairing: !c.security.require_pairing,
                },
              }))
            }
          />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Certificate Fingerprint</span>
            <span className="setting-description">
              SHA-256 fingerprint of this machine&apos;s certificate
            </span>
          </div>
          <code style={{ fontSize: 11, color: "var(--text-muted)" }}>
            Generating...
          </code>
        </div>
      </div>
      <div className="settings-group">
        <h3>Trusted Peers</h3>
        {config.security.trusted_fingerprints.length === 0 ? (
          <p style={{ color: "var(--text-muted)", fontSize: 13 }}>
            No trusted peers yet. Connect to a peer to establish trust.
          </p>
        ) : (
          <div>
            {config.security.trusted_fingerprints.map((fp) => (
              <div key={fp.fingerprint} className="setting-row">
                <div className="setting-info">
                  <span className="setting-label">{fp.hostname}</span>
                  <span className="setting-description">
                    {fp.fingerprint.substring(0, 32)}...
                  </span>
                </div>
                <span
                  style={{ fontSize: 12, color: "var(--text-muted)" }}
                >
                  {fp.first_seen}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function ScriptingSettings() {
  const [enabled, setEnabled] = useState(false);
  const [scriptsDir, setScriptsDir] = useState("~/.config/s-kvm/scripts");

  return (
    <div>
      <div className="settings-group">
        <h3>Rhai Scripting</h3>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Enable Scripting</span>
            <span className="setting-description">
              Allow Rhai scripts for automation
            </span>
          </div>
          <div
            className={`toggle ${enabled ? "active" : ""}`}
            onClick={() => setEnabled(!enabled)}
          />
        </div>
        <div className="setting-row">
          <div className="setting-info">
            <span className="setting-label">Scripts Directory</span>
            <span className="setting-description">
              Location of user scripts
            </span>
          </div>
          <input
            value={scriptsDir}
            onChange={(e) => setScriptsDir(e.target.value)}
            style={{ width: 250 }}
          />
        </div>
      </div>
    </div>
  );
}

export default Settings;
