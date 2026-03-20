import { useState } from "react";
import Sidebar from "./components/Sidebar";
import MonitorLayout from "./components/MonitorLayout";
import PeerList from "./components/PeerList";
import Settings from "./components/Settings";
import StatusBar from "./components/StatusBar";

type View = "layout" | "peers" | "settings";

function App() {
  const [currentView, setCurrentView] = useState<View>("layout");
  const [kvmActive, setKvmActive] = useState(false);

  return (
    <div className="app">
      <Sidebar
        currentView={currentView}
        onViewChange={setCurrentView}
        kvmActive={kvmActive}
        onToggleKvm={() => setKvmActive(!kvmActive)}
      />
      <main className="main-content">
        {currentView === "layout" && <MonitorLayout />}
        {currentView === "peers" && <PeerList />}
        {currentView === "settings" && <Settings />}
      </main>
      <StatusBar kvmActive={kvmActive} />
    </div>
  );
}

export default App;
