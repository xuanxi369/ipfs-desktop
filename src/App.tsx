import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

interface DaemonStatus {
  type: string;
  data?: any;
}

interface AppConfig {
  ipfs_path: string;
  api_addr: string;
  gateway_addr: string;
  language: string;
}

function App() {
  const [status, setStatus] = useState<DaemonStatus>({ type: "Stopped" });
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [error, setError] = useState<string>("");

  useEffect(() => {
    // 监听守护进程状态变化
    const unlisten = listen<DaemonStatus>("daemon-status-changed", (event) => {
      console.log("Status changed:", event.payload);
      setStatus(event.payload);
    });

    // 加载初始配置
    loadConfig();

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  async function loadConfig() {
    try {
      const cfg = await invoke<AppConfig>("get_config");
      setConfig(cfg);
      setError("");
    } catch (e) {
      setError(`Failed to load config: ${e}`);
    }
  }

  async function loadStatus() {
    try {
      const s = await invoke<DaemonStatus>("get_daemon_status");
      setStatus(s);
      setError("");
    } catch (e) {
      setError(`Failed to get status: ${e}`);
    }
  }

  async function startDaemon() {
    try {
      await invoke("start_daemon");
      await loadStatus();
      setError("");
    } catch (e) {
      setError(`Failed to start: ${e}`);
    }
  }

  async function stopDaemon() {
    try {
      await invoke("stop_daemon");
      await loadStatus();
      setError("");
    } catch (e) {
      setError(`Failed to stop: ${e}`);
    }
  }

  async function restartDaemon() {
    try {
      await invoke("restart_daemon");
      await loadStatus();
      setError("");
    } catch (e) {
      setError(`Failed to restart: ${e}`);
    }
  }

  const getStatusColor = () => {
    switch (status.type) {
      case "Running":
        return "#4caf50";
      case "Starting":
      case "Stopping":
        return "#ff9800";
      case "Failed":
        return "#f44336";
      default:
        return "#9e9e9e";
    }
  };

  return (
    <div className="container">
      <h1>IPFS Desktop (Rust)</h1>

      <div className="status-card">
        <h2>Daemon Status</h2>
        <div
          className="status-indicator"
          style={{ backgroundColor: getStatusColor() }}
        >
          {status.type}
        </div>
        {status.data && (
          <div className="status-details">
            {status.data.peer_id && <p>Peer ID: {status.data.peer_id}</p>}
            {status.data.api_addr && <p>API: {status.data.api_addr}</p>}
            {status.data.error && <p className="error">Error: {status.data.error}</p>}
          </div>
        )}
      </div>

      <div className="controls">
        <button onClick={startDaemon} disabled={status.type !== "Stopped"}>
          Start Daemon
        </button>
        <button onClick={stopDaemon} disabled={status.type === "Stopped"}>
          Stop Daemon
        </button>
        <button onClick={restartDaemon}>Restart Daemon</button>
        <button onClick={loadStatus}>Refresh Status</button>
      </div>

      {config && (
        <div className="config-card">
          <h2>Configuration</h2>
          <div className="config-item">
            <strong>IPFS Path:</strong> {config.ipfs_path}
          </div>
          <div className="config-item">
            <strong>API Address:</strong> {config.api_addr}
          </div>
          <div className="config-item">
            <strong>Gateway:</strong> {config.gateway_addr}
          </div>
          <div className="config-item">
            <strong>Language:</strong> {config.language}
          </div>
        </div>
      )}

      {error && <div className="error-message">{error}</div>}

      <footer>
        <p>Built with Tauri + React + Rust</p>
      </footer>
    </div>
  );
}

export default App;
