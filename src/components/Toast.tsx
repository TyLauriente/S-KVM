import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  type ReactNode,
} from "react";

interface Toast {
  id: number;
  message: string;
  type: "success" | "error" | "info";
}

interface ToastContextType {
  addToast: (message: string, type?: Toast["type"]) => void;
}

const ToastContext = createContext<ToastContextType>({
  addToast: () => {},
});

export function useToast() {
  return useContext(ToastContext);
}

let nextId = 0;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const addToast = useCallback(
    (message: string, type: Toast["type"] = "info") => {
      const id = nextId++;
      setToasts((prev) => [...prev, { id, message, type }]);
    },
    [],
  );

  const removeToast = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ addToast }}>
      {children}
      <div className="toast-container">
        {toasts.map((toast) => (
          <ToastItem key={toast.id} toast={toast} onDismiss={removeToast} />
        ))}
      </div>
      <style>{`
        .toast-container {
          position: fixed;
          bottom: 48px;
          right: 16px;
          display: flex;
          flex-direction: column;
          gap: 8px;
          z-index: 200;
          pointer-events: none;
        }
        .toast {
          padding: 12px 16px;
          border-radius: var(--radius);
          font-size: 13px;
          color: white;
          display: flex;
          align-items: center;
          gap: 8px;
          pointer-events: auto;
          animation: toast-in 0.3s ease;
          cursor: pointer;
          min-width: 280px;
          box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
        }
        .toast-success { background: #16a34a; }
        .toast-error { background: #dc2626; }
        .toast-info { background: #4f46e5; }
        @keyframes toast-in {
          from { opacity: 0; transform: translateY(16px); }
          to { opacity: 1; transform: translateY(0); }
        }
      `}</style>
    </ToastContext.Provider>
  );
}

function ToastItem({
  toast,
  onDismiss,
}: {
  toast: Toast;
  onDismiss: (id: number) => void;
}) {
  useEffect(() => {
    const timer = setTimeout(() => onDismiss(toast.id), 3000);
    return () => clearTimeout(timer);
  }, [toast.id, onDismiss]);

  const icons = { success: "\u2713", error: "\u2717", info: "\u2139" };

  return (
    <div
      className={`toast toast-${toast.type}`}
      onClick={() => onDismiss(toast.id)}
    >
      <span>{icons[toast.type]}</span>
      <span>{toast.message}</span>
    </div>
  );
}
