import React, { Component, type ErrorInfo, type ReactNode } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import "./i18n";

class RootErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("IPFS Desktop frontend crashed", error, info);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <main style={{ padding: 32, fontFamily: "Segoe UI, sans-serif", color: "#17252a" }}>
        <h1>IPFS Desktop failed to render</h1>
        <p>前端发生错误。Kubo 节点可能仍在后台运行。</p>
        <pre style={{ whiteSpace: "pre-wrap", padding: 16, background: "#eef5f4", borderRadius: 8 }}>
          {this.state.error.message || String(this.state.error)}
        </pre>
        <button onClick={() => { localStorage.removeItem("ipfs-benchmark-history"); location.reload(); }}>
          清理界面缓存并重新加载
        </button>
      </main>
    );
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <RootErrorBoundary><App /></RootErrorBoundary>
  </React.StrictMode>
);
