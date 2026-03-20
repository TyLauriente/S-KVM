import { useState, useRef, useCallback, useEffect } from "react";
import { safeInvoke } from "../mocks/tauriMock";
import { useToast } from "./Toast";
import { usePeers, useScreenLayout } from "../hooks/useTauriCommands";

interface Monitor {
  id: string;
  name: string;
  width: number;
  height: number;
  x: number;
  y: number;
  peer: string;
  peerId: string;
  isPrimary: boolean;
}

interface ContextMenuState {
  x: number;
  y: number;
  monitorId: string | null;
}

const BASE_SCALE = 0.15;
const SNAP_THRESHOLD = 30;
const MIN_ZOOM = 0.3;
const MAX_ZOOM = 3;

function snapToEdges(
  monitor: Monitor,
  others: Monitor[],
): { x: number; y: number } {
  let bestX = monitor.x;
  let bestY = monitor.y;
  let bestDx = SNAP_THRESHOLD;
  let bestDy = SNAP_THRESHOLD;

  for (const other of others) {
    if (other.id === monitor.id) continue;

    const mRight = monitor.x + monitor.width;
    const oRight = other.x + other.width;
    const mBottom = monitor.y + monitor.height;
    const oBottom = other.y + other.height;

    // X-axis snapping
    const xSnaps = [
      { delta: Math.abs(mRight - other.x), newX: other.x - monitor.width },
      { delta: Math.abs(monitor.x - oRight), newX: oRight },
      { delta: Math.abs(monitor.x - other.x), newX: other.x },
      { delta: Math.abs(mRight - oRight), newX: oRight - monitor.width },
    ];
    for (const snap of xSnaps) {
      if (snap.delta < bestDx) {
        bestDx = snap.delta;
        bestX = snap.newX;
      }
    }

    // Y-axis snapping
    const ySnaps = [
      { delta: Math.abs(mBottom - other.y), newY: other.y - monitor.height },
      { delta: Math.abs(monitor.y - oBottom), newY: oBottom },
      { delta: Math.abs(monitor.y - other.y), newY: other.y },
      { delta: Math.abs(mBottom - oBottom), newY: oBottom - monitor.height },
    ];
    for (const snap of ySnaps) {
      if (snap.delta < bestDy) {
        bestDy = snap.delta;
        bestY = snap.newY;
      }
    }
  }

  return { x: bestX, y: bestY };
}

interface EdgeConnection {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color: string;
}

function findEdgeConnections(monitors: Monitor[]): EdgeConnection[] {
  const connections: EdgeConnection[] = [];
  const colors = ["#22c55e", "#3b82f6", "#f59e0b", "#8b5cf6", "#ef4444"];
  let ci = 0;
  const TOUCH = 5;

  for (let i = 0; i < monitors.length; i++) {
    for (let j = i + 1; j < monitors.length; j++) {
      const a = monitors[i];
      const b = monitors[j];

      // Right of A → Left of B
      if (Math.abs(a.x + a.width - b.x) < TOUCH) {
        const top = Math.max(a.y, b.y);
        const bot = Math.min(a.y + a.height, b.y + b.height);
        if (top < bot) {
          connections.push({
            x1: a.x + a.width,
            y1: (top + bot) / 2,
            x2: b.x,
            y2: (top + bot) / 2,
            color: colors[ci++ % colors.length],
          });
        }
      }
      // Left of A → Right of B
      if (Math.abs(a.x - (b.x + b.width)) < TOUCH) {
        const top = Math.max(a.y, b.y);
        const bot = Math.min(a.y + a.height, b.y + b.height);
        if (top < bot) {
          connections.push({
            x1: a.x,
            y1: (top + bot) / 2,
            x2: b.x + b.width,
            y2: (top + bot) / 2,
            color: colors[ci++ % colors.length],
          });
        }
      }
      // Bottom of A → Top of B
      if (Math.abs(a.y + a.height - b.y) < TOUCH) {
        const left = Math.max(a.x, b.x);
        const right = Math.min(a.x + a.width, b.x + b.width);
        if (left < right) {
          connections.push({
            x1: (left + right) / 2,
            y1: a.y + a.height,
            x2: (left + right) / 2,
            y2: b.y,
            color: colors[ci++ % colors.length],
          });
        }
      }
      // Top of A → Bottom of B
      if (Math.abs(a.y - (b.y + b.height)) < TOUCH) {
        const left = Math.max(a.x, b.x);
        const right = Math.min(a.x + a.width, b.x + b.width);
        if (left < right) {
          connections.push({
            x1: (left + right) / 2,
            y1: a.y,
            x2: (left + right) / 2,
            y2: b.y + b.height,
            color: colors[ci++ % colors.length],
          });
        }
      }
    }
  }
  return connections;
}

function MonitorLayout() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const { addToast } = useToast();
  const { peers } = usePeers();
  const { updateLayout } = useScreenLayout();

  const [monitors, setMonitors] = useState<Monitor[]>([
    {
      id: "local-0",
      name: "Primary",
      width: 1920,
      height: 1080,
      x: 0,
      y: 0,
      peer: "This PC",
      peerId: "local",
      isPrimary: true,
    },
  ]);
  const [selectedMonitor, setSelectedMonitor] = useState<string | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [dragOffset, setDragOffset] = useState({ x: 0, y: 0 });
  const [canvasSize, setCanvasSize] = useState({ width: 800, height: 500 });
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [isPanning, setIsPanning] = useState(false);
  const [panStart, setPanStart] = useState({ x: 0, y: 0 });
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [showAddDialog, setShowAddDialog] = useState(false);

  // Load displays from backend on mount
  useEffect(() => {
    safeInvoke<Array<{ id: number; name: string; width: number; height: number; x: number; y: number; is_primary: boolean }>>("get_displays")
      .then((displays) => {
        if (displays.length > 0) {
          setMonitors(
            displays.map((d) => ({
              id: `local-${d.id}`,
              name: d.name,
              width: d.width,
              height: d.height,
              x: d.x,
              y: d.y,
              peer: "This PC",
              peerId: "local",
              isPrimary: d.is_primary,
            })),
          );
        }
      })
      .catch(() => {});
  }, []);

  // Canvas resize
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        setCanvasSize({
          width: entry.contentRect.width,
          height: entry.contentRect.height - 40,
        });
      }
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  // Coordinate transforms
  const toCanvas = useCallback(
    (wx: number, wy: number) => ({
      x: wx * BASE_SCALE * zoom + pan.x + canvasSize.width / 2,
      y: wy * BASE_SCALE * zoom + pan.y + canvasSize.height / 2,
    }),
    [zoom, pan, canvasSize],
  );

  const toWorld = useCallback(
    (cx: number, cy: number) => ({
      x: (cx - pan.x - canvasSize.width / 2) / (BASE_SCALE * zoom),
      y: (cy - pan.y - canvasSize.height / 2) / (BASE_SCALE * zoom),
    }),
    [zoom, pan, canvasSize],
  );

  // Drawing
  const drawCanvas = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Grid
    ctx.strokeStyle = "#1a1a24";
    ctx.lineWidth = 1;
    const gridSize = 40;
    const gx = ((pan.x + canvasSize.width / 2) % gridSize + gridSize) % gridSize;
    const gy = ((pan.y + canvasSize.height / 2) % gridSize + gridSize) % gridSize;
    for (let x = gx; x < canvas.width; x += gridSize) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, canvas.height);
      ctx.stroke();
    }
    for (let y = gy; y < canvas.height; y += gridSize) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(canvas.width, y);
      ctx.stroke();
    }

    // Edge connections
    const connections = findEdgeConnections(monitors);
    for (const conn of connections) {
      const p1 = toCanvas(conn.x1, conn.y1);
      const p2 = toCanvas(conn.x2, conn.y2);
      ctx.strokeStyle = conn.color;
      ctx.lineWidth = 3;
      ctx.setLineDash([6, 4]);
      ctx.beginPath();
      ctx.moveTo(p1.x, p1.y);
      ctx.lineTo(p2.x, p2.y);
      ctx.stroke();
      ctx.setLineDash([]);

      // Draw dots at endpoints
      for (const p of [p1, p2]) {
        ctx.fillStyle = conn.color;
        ctx.beginPath();
        ctx.arc(p.x, p.y, 4, 0, Math.PI * 2);
        ctx.fill();
      }
    }

    // Monitors
    for (const monitor of monitors) {
      const pos = toCanvas(monitor.x, monitor.y);
      const w = monitor.width * BASE_SCALE * zoom;
      const h = monitor.height * BASE_SCALE * zoom;
      const isSelected = selectedMonitor === monitor.id;

      // Shadow
      ctx.shadowColor = isSelected
        ? "rgba(99, 102, 241, 0.3)"
        : "rgba(0, 0, 0, 0.3)";
      ctx.shadowBlur = isSelected ? 12 : 6;
      ctx.shadowOffsetY = 2;

      // Body
      ctx.fillStyle = isSelected ? "#2d2d5f" : "#252532";
      ctx.strokeStyle = isSelected ? "#6366f1" : "#3a3a4a";
      ctx.lineWidth = isSelected ? 2 : 1;
      ctx.beginPath();
      ctx.roundRect(pos.x, pos.y, w, h, 6);
      ctx.fill();
      ctx.stroke();

      ctx.shadowColor = "transparent";
      ctx.shadowBlur = 0;
      ctx.shadowOffsetY = 0;

      // Primary indicator
      if (monitor.isPrimary) {
        ctx.fillStyle = "#6366f1";
        ctx.beginPath();
        ctx.roundRect(pos.x + w / 2 - 20, pos.y + 4, 40, 3, 2);
        ctx.fill();
      }

      // Label
      const fontSize = Math.max(10, 13 * zoom);
      ctx.fillStyle = "#e4e4ef";
      ctx.font = `bold ${fontSize}px Inter, sans-serif`;
      ctx.textAlign = "center";
      ctx.fillText(monitor.name, pos.x + w / 2, pos.y + h / 2 - 8 * zoom);

      // Resolution
      ctx.fillStyle = "#9999aa";
      ctx.font = `${Math.max(8, 11 * zoom)}px Inter, sans-serif`;
      ctx.fillText(
        `${monitor.width}x${monitor.height}`,
        pos.x + w / 2,
        pos.y + h / 2 + 8 * zoom,
      );

      // Peer name
      ctx.fillStyle = monitor.peerId === "local" ? "#6366f1" : "#22c55e";
      ctx.font = `${Math.max(7, 10 * zoom)}px Inter, sans-serif`;
      ctx.fillText(monitor.peer, pos.x + w / 2, pos.y + h / 2 + 22 * zoom);
    }
  }, [monitors, selectedMonitor, canvasSize, zoom, pan, toCanvas]);

  useEffect(() => {
    drawCanvas();
  }, [drawCanvas]);

  // Mouse handlers
  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    setContextMenu(null);

    if (e.button === 1 || (e.button === 0 && e.ctrlKey)) {
      // Middle click or Ctrl+click: start panning
      setIsPanning(true);
      setPanStart({ x: e.clientX - pan.x, y: e.clientY - pan.y });
      e.preventDefault();
      return;
    }

    if (e.button !== 0) return;

    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    // Hit test monitors (reverse order for top-most first)
    for (let i = monitors.length - 1; i >= 0; i--) {
      const monitor = monitors[i];
      const pos = toCanvas(monitor.x, monitor.y);
      const w = monitor.width * BASE_SCALE * zoom;
      const h = monitor.height * BASE_SCALE * zoom;

      if (mx >= pos.x && mx <= pos.x + w && my >= pos.y && my <= pos.y + h) {
        setSelectedMonitor(monitor.id);
        setDragging(monitor.id);
        setDragOffset({ x: mx - pos.x, y: my - pos.y });
        return;
      }
    }

    setSelectedMonitor(null);
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (isPanning) {
      setPan({
        x: e.clientX - panStart.x,
        y: e.clientY - panStart.y,
      });
      return;
    }

    if (!dragging) return;
    const canvas = canvasRef.current;
    if (!canvas) return;

    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    const world = toWorld(mx - dragOffset.x, my - dragOffset.y);
    const gridSnapped = {
      x: Math.round(world.x / 10) * 10,
      y: Math.round(world.y / 10) * 10,
    };

    setMonitors((prev) => {
      const updated = prev.map((m) =>
        m.id === dragging ? { ...m, x: gridSnapped.x, y: gridSnapped.y } : m,
      );
      // Edge snap
      const draggedIdx = updated.findIndex((m) => m.id === dragging);
      if (draggedIdx !== -1) {
        const snapped = snapToEdges(updated[draggedIdx], updated);
        updated[draggedIdx] = {
          ...updated[draggedIdx],
          x: snapped.x,
          y: snapped.y,
        };
      }
      return updated;
    });
  };

  const handleMouseUp = () => {
    setDragging(null);
    setIsPanning(false);
  };

  const handleWheel = (e: React.WheelEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    setZoom((z) => Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, z * delta)));
  };

  const handleContextMenu = (e: React.MouseEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const canvas = canvasRef.current;
    if (!canvas) return;

    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    let targetMonitor: string | null = null;
    for (let i = monitors.length - 1; i >= 0; i--) {
      const monitor = monitors[i];
      const pos = toCanvas(monitor.x, monitor.y);
      const w = monitor.width * BASE_SCALE * zoom;
      const h = monitor.height * BASE_SCALE * zoom;
      if (mx >= pos.x && mx <= pos.x + w && my >= pos.y && my <= pos.y + h) {
        targetMonitor = monitor.id;
        break;
      }
    }

    setContextMenu({ x: e.clientX, y: e.clientY, monitorId: targetMonitor });
    if (targetMonitor) setSelectedMonitor(targetMonitor);
  };

  // Actions
  const autoArrange = () => {
    setMonitors((prev) => {
      let xOffset = 0;
      return prev.map((m) => {
        const updated = { ...m, x: xOffset, y: 0 };
        xOffset += m.width;
        return updated;
      });
    });
    setPan({ x: 0, y: 0 });
    setZoom(1);
  };

  const resetView = () => {
    setPan({ x: 0, y: 0 });
    setZoom(1);
  };

  const saveLayout = async () => {
    try {
      await updateLayout({ links: [] });
      addToast("Layout saved", "success");
    } catch {
      addToast("Failed to save layout", "error");
    }
  };

  const addPeerMonitor = (peerHostname: string, peerId: string) => {
    const maxX = monitors.reduce(
      (max, m) => Math.max(max, m.x + m.width),
      0,
    );
    setMonitors((prev) => [
      ...prev,
      {
        id: `peer-${peerId}-${Date.now()}`,
        name: `${peerHostname} Display`,
        width: 1920,
        height: 1080,
        x: maxX,
        y: 0,
        peer: peerHostname,
        peerId,
        isPrimary: false,
      },
    ]);
    setShowAddDialog(false);
    addToast(`Added ${peerHostname} monitor`, "success");
  };

  const removeMonitor = (id: string) => {
    setMonitors((prev) => prev.filter((m) => m.id !== id));
    setSelectedMonitor(null);
    setContextMenu(null);
  };

  const setPrimary = (id: string) => {
    setMonitors((prev) =>
      prev.map((m) => ({ ...m, isPrimary: m.id === id })),
    );
    setContextMenu(null);
  };

  // Close context menu on outside click
  useEffect(() => {
    if (!contextMenu) return;
    const handler = () => setContextMenu(null);
    window.addEventListener("click", handler);
    return () => window.removeEventListener("click", handler);
  }, [contextMenu]);

  // Escape key closes context menu and dialogs
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setContextMenu(null);
        setShowAddDialog(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const connectedPeers = peers.filter(
    (p) => p.state === "Connected" || p.state === "Active",
  );

  return (
    <div className="monitor-layout" ref={containerRef}>
      <div className="section-header">
        <h2 className="section-title">Monitor Layout</h2>
        <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
          <div className="zoom-controls">
            <button
              className="zoom-btn"
              onClick={() =>
                setZoom((z) => Math.max(MIN_ZOOM, z * 0.8))
              }
              title="Zoom out"
            >
              -
            </button>
            <span className="zoom-level">{Math.round(zoom * 100)}%</span>
            <button
              className="zoom-btn"
              onClick={() =>
                setZoom((z) => Math.min(MAX_ZOOM, z * 1.2))
              }
              title="Zoom in"
            >
              +
            </button>
            <button className="zoom-btn" onClick={resetView} title="Reset view">
              {"\u2302"}
            </button>
          </div>
          <button
            className="btn btn-secondary"
            onClick={() => setShowAddDialog(true)}
          >
            Add Monitor
          </button>
          <button className="btn btn-secondary" onClick={autoArrange}>
            Auto Arrange
          </button>
          <button className="btn btn-primary" onClick={saveLayout}>
            Save Layout
          </button>
        </div>
      </div>

      <div className="canvas-container">
        <canvas
          ref={canvasRef}
          width={canvasSize.width}
          height={canvasSize.height}
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onMouseLeave={handleMouseUp}
          onWheel={handleWheel}
          onContextMenu={handleContextMenu}
        />
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <div
          className="ctx-menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          {contextMenu.monitorId ? (
            <>
              <button
                className="ctx-item"
                onClick={() => setPrimary(contextMenu.monitorId!)}
              >
                Set as Primary
              </button>
              <button
                className="ctx-item"
                onClick={() => {
                  setSelectedMonitor(contextMenu.monitorId);
                  setContextMenu(null);
                }}
              >
                Properties
              </button>
              {monitors.find((m) => m.id === contextMenu.monitorId)?.peerId !==
                "local" && (
                <button
                  className="ctx-item ctx-danger"
                  onClick={() => removeMonitor(contextMenu.monitorId!)}
                >
                  Remove
                </button>
              )}
            </>
          ) : (
            <>
              <button
                className="ctx-item"
                onClick={() => {
                  setShowAddDialog(true);
                  setContextMenu(null);
                }}
              >
                Add Monitor
              </button>
              <button
                className="ctx-item"
                onClick={() => {
                  autoArrange();
                  setContextMenu(null);
                }}
              >
                Auto Arrange
              </button>
              <button
                className="ctx-item"
                onClick={() => {
                  resetView();
                  setContextMenu(null);
                }}
              >
                Reset View
              </button>
            </>
          )}
        </div>
      )}

      {/* Properties Panel */}
      {selectedMonitor && (
        <div className="monitor-properties card">
          <h3>Monitor Properties</h3>
          {monitors
            .filter((m) => m.id === selectedMonitor)
            .map((m) => (
              <div key={m.id} className="props-grid">
                <label>Name</label>
                <input
                  value={m.name}
                  onChange={(e) =>
                    setMonitors((prev) =>
                      prev.map((mon) =>
                        mon.id === m.id
                          ? { ...mon, name: e.target.value }
                          : mon,
                      ),
                    )
                  }
                />
                <label>Peer</label>
                <span className="prop-value">{m.peer}</span>
                <label>Resolution</label>
                <span className="prop-value">
                  {m.width}x{m.height}
                </span>
                <label>Position</label>
                <span className="prop-value">
                  ({m.x}, {m.y})
                </span>
                <label>Primary</label>
                <span className="prop-value">{m.isPrimary ? "Yes" : "No"}</span>
              </div>
            ))}
        </div>
      )}

      {/* Add Monitor Dialog */}
      {showAddDialog && (
        <div
          className="dialog-overlay"
          onClick={() => setShowAddDialog(false)}
        >
          <div className="dialog card" onClick={(e) => e.stopPropagation()}>
            <h3>Add Remote Monitor</h3>
            {connectedPeers.length === 0 ? (
              <p
                style={{
                  color: "var(--text-muted)",
                  fontSize: 14,
                  lineHeight: 1.5,
                }}
              >
                No peers connected. Connect to a peer first in the Peers tab,
                then add their monitors here.
              </p>
            ) : (
              <div className="add-monitor-list">
                {connectedPeers.map((peer) => (
                  <button
                    key={peer.id}
                    className="add-monitor-item"
                    onClick={() =>
                      addPeerMonitor(peer.hostname, peer.id)
                    }
                  >
                    <span className="peer-dot active" />
                    <span>{peer.hostname}</span>
                    <span className="peer-os-badge">{peer.os}</span>
                  </button>
                ))}
              </div>
            )}
            <div className="dialog-actions">
              <button
                className="btn btn-secondary"
                onClick={() => setShowAddDialog(false)}
              >
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}

      <style>{`
        .monitor-layout {
          display: flex;
          flex-direction: column;
          height: 100%;
        }
        .canvas-container {
          flex: 1;
          background: var(--bg-secondary);
          border: 1px solid var(--border);
          border-radius: var(--radius);
          overflow: hidden;
          position: relative;
        }
        .canvas-container canvas {
          display: block;
          cursor: grab;
        }
        .canvas-container canvas:active {
          cursor: grabbing;
        }
        .zoom-controls {
          display: flex;
          align-items: center;
          gap: 4px;
          background: var(--bg-tertiary);
          border: 1px solid var(--border);
          border-radius: var(--radius-sm);
          padding: 2px;
        }
        .zoom-btn {
          width: 28px;
          height: 28px;
          display: flex;
          align-items: center;
          justify-content: center;
          background: transparent;
          color: var(--text-primary);
          border-radius: 3px;
          font-size: 14px;
          font-weight: 600;
        }
        .zoom-btn:hover {
          background: var(--bg-hover);
        }
        .zoom-level {
          font-size: 12px;
          color: var(--text-muted);
          min-width: 40px;
          text-align: center;
        }
        .monitor-properties {
          margin-top: 16px;
        }
        .monitor-properties h3 {
          margin-bottom: 12px;
          font-size: 14px;
          color: var(--text-secondary);
        }
        .props-grid {
          display: grid;
          grid-template-columns: 100px 1fr;
          gap: 8px;
          align-items: center;
        }
        .props-grid label {
          font-size: 13px;
          color: var(--text-muted);
        }
        .prop-value {
          font-size: 13px;
          color: var(--text-primary);
        }
        .ctx-menu {
          position: fixed;
          background: var(--bg-secondary);
          border: 1px solid var(--border);
          border-radius: var(--radius);
          padding: 4px;
          z-index: 150;
          min-width: 160px;
          box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
        }
        .ctx-item {
          display: block;
          width: 100%;
          padding: 8px 12px;
          background: transparent;
          color: var(--text-primary);
          text-align: left;
          font-size: 13px;
          border-radius: var(--radius-sm);
        }
        .ctx-item:hover {
          background: var(--bg-hover);
        }
        .ctx-danger {
          color: var(--danger);
        }
        .ctx-danger:hover {
          background: rgba(239, 68, 68, 0.1);
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
        .dialog h3 { font-size: 18px; }
        .dialog-actions {
          display: flex;
          gap: 8px;
          justify-content: flex-end;
        }
        .add-monitor-list {
          display: flex;
          flex-direction: column;
          gap: 4px;
        }
        .add-monitor-item {
          display: flex;
          align-items: center;
          gap: 10px;
          padding: 10px 12px;
          background: var(--bg-tertiary);
          border-radius: var(--radius-sm);
          color: var(--text-primary);
          text-align: left;
          font-size: 14px;
        }
        .add-monitor-item:hover {
          background: var(--bg-hover);
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
        .peer-os-badge {
          font-size: 11px;
          color: var(--text-muted);
          background: var(--bg-primary);
          padding: 2px 6px;
          border-radius: 3px;
          margin-left: auto;
        }
      `}</style>
    </div>
  );
}

export default MonitorLayout;
