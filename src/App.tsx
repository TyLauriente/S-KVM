import { useState } from "react";
import Sidebar from "./components/Sidebar";
import Dashboard from "./components/Dashboard";
import MonitorLayout from "./components/MonitorLayout";
import PeerList from "./components/PeerList";
import Settings from "./components/Settings";
import StatusBar from "./components/StatusBar";
import { ToastProvider } from "./components/Toast";
import { useKvmStatus } from "./hooks/useTauriCommands";

type View = "dashboard" | "layout" | "peers" | "settings";

function App() {
  const [currentView, setCurrentView] = useState<View>("dashboard");
  const { status, toggleKvm } = useKvmStatus();

  return (
    <ToastProvider>
      <div className="app">
        <Sidebar
          currentView={currentView}
          onViewChange={setCurrentView}
          kvmActive={status.active}
          onToggleKvm={toggleKvm}
        />
        <main className="main-content">
          {currentView === "dashboard" && <Dashboard />}
          {currentView === "layout" && <MonitorLayout />}
          {currentView === "peers" && <PeerList />}
          {currentView === "settings" && <Settings />}
        </main>
        <StatusBar kvmActive={status.active} connectedPeers={status.connected_peers} />
      </div>
    </ToastProvider>
  );
}

export default App;
