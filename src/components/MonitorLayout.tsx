import { useState, useRef, useCallback, useEffect } from "react";

interface Monitor {
  id: string;
  name: string;
  width: number;
  height: number;
  x: number;
  y: number;
  peer: string;
  isPrimary: boolean;
}

function MonitorLayout() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [monitors, setMonitors] = useState<Monitor[]>([
    { id: "local-0", name: "Primary", width: 1920, height: 1080, x: 0, y: 0, peer: "This PC", isPrimary: true },
  ]);
  const [selectedMonitor, setSelectedMonitor] = useState<string | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [dragOffset, setDragOffset] = useState({ x: 0, y: 0 });
  const [canvasSize, setCanvasSize] = useState({ width: 800, height: 500 });

  const scale = 0.15;
  const padding = 60;

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

  const drawMonitors = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Center the layout
    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;

    // Draw grid
    ctx.strokeStyle = "#1a1a24";
    ctx.lineWidth = 1;
    for (let x = 0; x < canvas.width; x += 40) {
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, canvas.height);
      ctx.stroke();
    }
    for (let y = 0; y < canvas.height; y += 40) {
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(canvas.width, y);
      ctx.stroke();
    }

    // Draw monitors
    monitors.forEach((monitor) => {
      const x = centerX + monitor.x * scale + padding;
      const y = centerY + monitor.y * scale + padding;
      const w = monitor.width * scale;
      const h = monitor.height * scale;

      const isSelected = selectedMonitor === monitor.id;

      // Monitor body
      ctx.fillStyle = isSelected ? "#2d2d5f" : "#252532";
      ctx.strokeStyle = isSelected ? "#6366f1" : "#3a3a4a";
      ctx.lineWidth = isSelected ? 2 : 1;
      ctx.beginPath();
      ctx.roundRect(x, y, w, h, 6);
      ctx.fill();
      ctx.stroke();

      // Monitor label
      ctx.fillStyle = "#e4e4ef";
      ctx.font = "bold 13px Inter, sans-serif";
      ctx.textAlign = "center";
      ctx.fillText(monitor.name, x + w / 2, y + h / 2 - 8);

      // Resolution
      ctx.fillStyle = "#9999aa";
      ctx.font = "11px Inter, sans-serif";
      ctx.fillText(`${monitor.width}x${monitor.height}`, x + w / 2, y + h / 2 + 8);

      // Peer name
      ctx.fillStyle = "#6366f1";
      ctx.font = "10px Inter, sans-serif";
      ctx.fillText(monitor.peer, x + w / 2, y + h / 2 + 24);
    });
  }, [monitors, selectedMonitor, canvasSize]);

  useEffect(() => {
    drawMonitors();
  }, [drawMonitors]);

  const handleCanvasMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;

    for (const monitor of monitors) {
      const x = centerX + monitor.x * scale + padding;
      const y = centerY + monitor.y * scale + padding;
      const w = monitor.width * scale;
      const h = monitor.height * scale;

      if (mx >= x && mx <= x + w && my >= y && my <= y + h) {
        setSelectedMonitor(monitor.id);
        setDragging(monitor.id);
        setDragOffset({ x: mx - x, y: my - y });
        return;
      }
    }

    setSelectedMonitor(null);
  };

  const handleCanvasMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!dragging) return;
    const canvas = canvasRef.current;
    if (!canvas) return;

    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;

    setMonitors((prev) =>
      prev.map((m) =>
        m.id === dragging
          ? {
              ...m,
              x: Math.round((mx - dragOffset.x - centerX - padding) / scale / 10) * 10,
              y: Math.round((my - dragOffset.y - centerY - padding) / scale / 10) * 10,
            }
          : m,
      ),
    );
  };

  const handleCanvasMouseUp = () => {
    setDragging(null);
  };

  return (
    <div className="monitor-layout" ref={containerRef}>
      <div className="section-header">
        <h2 className="section-title">Monitor Layout</h2>
        <div style={{ display: "flex", gap: "8px" }}>
          <button className="btn btn-secondary">Auto Arrange</button>
          <button className="btn btn-primary">Save Layout</button>
        </div>
      </div>

      <div className="canvas-container">
        <canvas
          ref={canvasRef}
          width={canvasSize.width}
          height={canvasSize.height}
          onMouseDown={handleCanvasMouseDown}
          onMouseMove={handleCanvasMouseMove}
          onMouseUp={handleCanvasMouseUp}
          onMouseLeave={handleCanvasMouseUp}
        />
      </div>

      {selectedMonitor && (
        <div className="monitor-properties card">
          <h3>Monitor Properties</h3>
          {monitors
            .filter((m) => m.id === selectedMonitor)
            .map((m) => (
              <div key={m.id} className="props-grid">
                <label>Name</label>
                <input value={m.name} onChange={(e) =>
                  setMonitors((prev) =>
                    prev.map((mon) => mon.id === m.id ? { ...mon, name: e.target.value } : mon)
                  )
                } />
                <label>Peer</label>
                <span className="prop-value">{m.peer}</span>
                <label>Resolution</label>
                <span className="prop-value">{m.width}x{m.height}</span>
                <label>Position</label>
                <span className="prop-value">({m.x}, {m.y})</span>
              </div>
            ))}
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
      `}</style>
    </div>
  );
}

export default MonitorLayout;
