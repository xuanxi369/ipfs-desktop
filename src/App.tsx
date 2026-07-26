import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import "./App.css";

// ── 类型定义 ──

interface DaemonStatus {
  type: string;
  data?: {
    pid?: number;
    peer_id?: string;
    api_addr?: string;
    error?: string;
  };
}

interface AppConfig {
  ipfs_path: string | null;
  api_addr: string;
  gateway_addr: string;
  daemon_flags: string[];
  auto_launch: boolean;
  auto_gc: boolean;
}

interface AddResult {
  Hash: string;
  Size: string;
  Name: string;
}

interface StructuredError {
  BinaryNotFound?: null;
  BinaryVerificationFailed?: string;
  ProcessStartFailed?: string;
  ProcessExitedUnexpectedly?: null;
  ProcessStopFailed?: string;
  InvalidState?: null;
  ApiError?: string;
  ApiConnectionFailed?: { addr: string; source: string };
  ApiParseError?: string;
  ConfigError?: string;
  IoError?: string;
}

interface PinEntry {
  Cid: string;
  Type: string;
}

interface PinList {
  pins: PinEntry[];
}

interface DashboardStats {
  node_id: { id: string; agent_version: string } | null;
  version: string | null;
  repo: { num_objects: number; repo_size: number } | null;
  peers: { peers: { Peer: string; Addr: string }[] } | null;
  bandwidth: { total_in: number; total_out: number; rate_in: number; rate_out: number } | null;
  bitswap: { blocks_received: number; blocks_sent: number; data_received: number; data_sent: number } | null;
  pin_count: number;
}

interface DownloadProgress {
  cid: string;
  loaded: number;
  total: number | null;
}

interface UploadProgress {
  name: string;
  loaded: number;
  total: number;
}

// ── 工具函数 ──

function formatError(e: unknown): string {
  if (typeof e === "string") return e;
  if (typeof e === "object" && e !== null) {
    const err = e as StructuredError;
    if (err.BinaryNotFound !== undefined) return "IPFS binary not found. Please install Kubo.";
    if (err.BinaryVerificationFailed) return `Binary verification failed: ${err.BinaryVerificationFailed}`;
    if (err.ProcessStartFailed) return `Process start failed: ${err.ProcessStartFailed}`;
    if (err.ProcessExitedUnexpectedly !== undefined) return "Daemon process exited unexpectedly.";
    if (err.ProcessStopFailed) return `Process stop failed: ${err.ProcessStopFailed}`;
    if (err.InvalidState !== undefined) return "Invalid daemon state for this operation.";
    if (err.ApiError) return `API error: ${err.ApiError}`;
    if (err.ApiConnectionFailed) return `API connection failed at ${err.ApiConnectionFailed.addr}: ${err.ApiConnectionFailed.source}`;
    if (err.ApiParseError) return `API parse error: ${err.ApiParseError}`;
    if (err.ConfigError) return `Configuration error: ${err.ConfigError}`;
    if (err.IoError) return `I/O error: ${err.IoError}`;
    return JSON.stringify(e);
  }
  return String(e);
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

function formatRate(bytesPerSec: number): string {
  return formatBytes(bytesPerSec) + "/s";
}

type TabName = "dashboard" | "webui" | "files" | "pins" | "ipns";

interface DashboardTick {
  peers: { peers: { Peer: string; Addr: string }[] } | null;
  bandwidth: { total_in: number; total_out: number; rate_in: number; rate_out: number } | null;
  bitswap: { blocks_received: number; blocks_sent: number; data_received: number; data_sent: number } | null;
  repo: { num_objects: number; repo_size: number } | null;
}

// ── 主组件 ──

function App() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<DaemonStatus>({ type: "Stopped" });
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [error, setError] = useState<string>("");
  const [activeTab, setActiveTab] = useState<TabName>("dashboard");
  const [uploads, setUploads] = useState<AddResult[]>([]);
  const [uploading, setUploading] = useState(false);

  // ── A1 下载状态 ──
  const [downloadCid, setDownloadCid] = useState("");
  const [downloadProgress, setDownloadProgress] = useState<DownloadProgress | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [catResult, setCatResult] = useState<string>("");

  // ── A2 Pin 状态 ──
  const [pinList, setPinList] = useState<PinEntry[]>([]);
  const [pinLoading, setPinLoading] = useState(false);
  const [pinCid, setPinCid] = useState("");

  // ── A3 仪表盘状态 ──
  const [dashboard, setDashboard] = useState<DashboardStats | null>(null);
  const [dashLoading, setDashLoading] = useState(false);

  // ── 上传进度 ──
  const [uploadProgress, setUploadProgress] = useState<UploadProgress | null>(null);

  // ── Phase 2: IPNS 状态 ──
  const [ipnsCid, setIpnsCid] = useState("");
  const [ipnsKeyName, setIpnsKeyName] = useState("self");
  const [ipnsLifetime, setIpnsLifetime] = useState("24h");
  const [ipnsResolveName, setIpnsResolveName] = useState("");
  const [ipnsResolveResult, setIpnsResolveResult] = useState("");
  const [ipnsPublishResult, setIpnsPublishResult] = useState("");
  const [keyList, setKeyList] = useState<{public_key:string;ipns_name:string;label:string}[]>([]);
  const [newKeyLabel, setNewKeyLabel] = useState("");

  // ── 缓存指示器 ──
  const [cacheHit, setCacheHit] = useState(false);

  // ── Phase 3: 代理统计 ──
  const [proxyStats, setProxyStats] = useState<{total_requests:number;cache_hits:number;api_calls:number;circuit_open_count:number;avg_latency_ms:number} | null>(null);

  // ── Phase 3: 离线队列 ──
  const [offlineCount, setOfflineCount] = useState(0);

  // ── Phase 3: 带宽控制 ──
  const [bwConfig, setBwConfig] = useState<{max_connections:number;max_streams:number;upload_limit:number;download_limit:number;enabled:boolean}>({max_connections:600,max_streams:2048,upload_limit:0,download_limit:0,enabled:true});
  const [bwStatus, setBwStatus] = useState<{rate_in:number;rate_out:number;total_in:number;total_out:number} | null>(null);

  // ── Phase 4: 后端切换 ──
  const [activeBackend, setActiveBackend] = useState("kubo");
  const [backendCaps, setBackendCaps] = useState<Record<string,unknown> | null>(null);
  const [benchResult, setBenchResult] = useState<Record<string,unknown> | null>(null);
  const [benchRunning, setBenchRunning] = useState(false);
  const [compatResult, setCompatResult] = useState<Record<string,unknown> | null>(null);
  const [compatRunning, setCompatRunning] = useState(false);

  const webuiUrl = config ? `${config.api_addr}/webui` : "";

  // ── 初始化 ──
  useEffect(() => {
    let mounted = true;
    const unlistenStatus = listen<DaemonStatus>("daemon-status-changed", (event) => {
      if (!mounted) return;
      setStatus(event.payload);
    });
    const unlistenDownload = listen<DownloadProgress>("download-progress", (event) => {
      if (!mounted) return;
      setDownloadProgress(event.payload);
    });
    const unlistenUpload = listen<UploadProgress>("upload-progress", (event) => {
      if (!mounted) return;
      setUploadProgress(event.payload);
    });
    const unlistenDash = listen<DashboardTick>("dashboard-tick", (event) => {
      if (!mounted) return;
      const tick = event.payload;
      setDashboard((prev) => prev ? { ...prev, ...tick } : ({ node_id: null, version: null, repo: tick.repo || null, peers: tick.peers || null, bandwidth: tick.bandwidth || null, bitswap: tick.bitswap || null, pin_count: 0 } as DashboardStats));
      setCacheHit(false);
      // Phase 3: 更新带宽实时状态
      if (tick.bandwidth) {
        setBwStatus({ rate_in: tick.bandwidth.rate_in, rate_out: tick.bandwidth.rate_out, total_in: tick.bandwidth.total_in, total_out: tick.bandwidth.total_out });
      }
    });
    const unlistenReplay = listen<{success:number;failed:number;remaining:number}>("replay-progress", (event) => {
      if (!mounted) return;
      setOfflineCount(event.payload.remaining);
    });
    loadConfig();
    loadStatus();
    return () => {
      mounted = false;
      unlistenStatus.then((fn) => fn());
      unlistenDownload.then((fn) => fn());
      unlistenUpload.then((fn) => fn());
      unlistenDash.then((fn) => fn());
      unlistenReplay.then((fn) => fn());
    };
  }, []);

  // ── 数据加载 ──
  async function loadConfig() {
    try { const cfg = await invoke<AppConfig>("get_config"); setConfig(cfg); setError(""); }
    catch (e) { setError(`Failed to load config: ${formatError(e)}`); }
  }
  async function loadStatus() {
    try { const s = await invoke<DaemonStatus>("get_daemon_status"); setStatus(s); setError(""); }
    catch (e) { setError(`Failed to get status: ${formatError(e)}`); }
  }

  // ── 守护进程操作 ──
  async function startDaemon() {
    try { await invoke("start_daemon"); await loadStatus(); setError(""); }
    catch (e) { setError(`Failed to start: ${formatError(e)}`); }
  }
  async function stopDaemon() {
    try { await invoke("stop_daemon"); await loadStatus(); setError(""); }
    catch (e) { setError(`Failed to stop: ${formatError(e)}`); }
  }
  async function restartDaemon() {
    try { await invoke("restart_daemon"); await loadStatus(); setError(""); }
    catch (e) { setError(`Failed to restart: ${formatError(e)}`); }
  }
  async function openWebui() {
    try { await invoke("open_webui"); setError(""); }
    catch (e) { setError(`Failed to open WebUI: ${formatError(e)}`); }
  }

  // ── A1 下载 ──
  async function downloadByCid() {
    if (!downloadCid.trim()) return;
    try {
      setDownloading(true);
      setDownloadProgress({ cid: downloadCid, loaded: 0, total: null });
      const savePath = await save({ defaultPath: downloadCid, title: t("saveDownloadAs") });
      if (!savePath) { setDownloading(false); return; }
      await invoke("download_file", { cid: downloadCid.trim(), savePath });
      setError("");
    } catch (e) {
      setError(`Download failed: ${formatError(e)}`);
    } finally {
      setDownloading(false);
    }
  }

  async function catByCid() {
    if (!downloadCid.trim()) return;
    try {
      const data = await invoke<number[]>("cat_file", { cid: downloadCid.trim() });
      const text = new TextDecoder().decode(new Uint8Array(data));
      setCatResult(text.slice(0, 5000));
      setError("");
    } catch (e) {
      setError(`Cat failed: ${formatError(e)}`);
    }
  }

  // ── A2 Pin 管理 ──
  async function loadPins() {
    try {
      setPinLoading(true);
      const result = await invoke<PinList>("get_pin_list");
      setPinList(result.pins || []);
      setError("");
    } catch (e) {
      setError(`Pin list failed: ${formatError(e)}`);
    } finally {
      setPinLoading(false);
    }
  }

  async function addPinByCid() {
    if (!pinCid.trim()) return;
    try {
      await invoke("add_pin", { cid: pinCid.trim() });
      setPinCid("");
      await loadPins();
    } catch (e) {
      setError(`Pin add failed: ${formatError(e)}`);
    }
  }

  async function removePinByCid(cid: string) {
    try {
      await invoke("remove_pin", { cid });
      await loadPins();
    } catch (e) {
      setError(`Pin remove failed: ${formatError(e)}`);
    }
  }

  // ── A3 仪表盘（优先使用缓存） ──
  async function loadDashboard() {
    try {
      setDashLoading(true);
      // Phase 2: 优先从缓存读取，毫秒级响应
      const stats = await invoke<DashboardStats>("get_cached_dashboard");
      setDashboard(stats);
      setCacheHit(true);
      // Phase 3: 同时加载代理统计
      try {
        const ps = await invoke<{total_requests:number;cache_hits:number;api_calls:number;circuit_open_count:number;avg_latency_ms:number}>("get_proxy_stats");
        setProxyStats(ps);
      } catch { /* proxy stats are optional */ }
      setError("");
    } catch (e) {
      // 缓存失败则回退到直接 API 查询
      try {
        const stats = await invoke<DashboardStats>("get_dashboard_stats");
        setDashboard(stats);
        setError("");
      } catch (e2) {
        setError(`Dashboard load failed: ${formatError(e2)}`);
      }
    } finally {
      setDashLoading(false);
    }
  }

  // ── Phase 2: IPNS ──
  async function publishIpns() {
    if (!ipnsCid.trim()) return;
    try {
      const result = await invoke<{Name:string;Value:string}>("ipns_publish", {
        cid: ipnsCid.trim(),
        keyName: ipnsKeyName.trim() || "self",
        lifetime: ipnsLifetime,
      });
      setIpnsPublishResult(`${result.Name} → ${result.Value}`);
      setError("");
    } catch (e) {
      setError(`IPNS publish failed: ${formatError(e)}`);
    }
  }

  async function resolveIpns() {
    if (!ipnsResolveName.trim()) return;
    try {
      const result = await invoke<{Path:string}>("ipns_resolve", { name: ipnsResolveName.trim() });
      setIpnsResolveResult(result.Path);
      setError("");
    } catch (e) {
      setError(`IPNS resolve failed: ${formatError(e)}`);
    }
  }

  async function generateNewKey() {
    if (!newKeyLabel.trim()) return;
    try {
      await invoke("generate_key", { label: newKeyLabel.trim() });
      setNewKeyLabel("");
      await loadKeyList();
      setError("");
    } catch (e) {
      setError(`Key generation failed: ${formatError(e)}`);
    }
  }

  async function loadKeyList() {
    try {
      const keys = await invoke<{public_key:string;ipns_name:string;label:string}[]>("list_keys");
      setKeyList(keys || []);
    } catch (e) {
      setError(`Key list failed: ${formatError(e)}`);
    }
  }

  async function deleteKeyByLabel(label: string) {
    try {
      await invoke("delete_key", { label });
      await loadKeyList();
    } catch (e) {
      setError(`Key delete failed: ${formatError(e)}`);
    }
  }

  // ── 文件上传 ──
  const selectAndUpload = useCallback(async () => {
    try {
      const selected = await open({ multiple: true, title: t("selectFiles") });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      setUploading(true);
      setUploadProgress(null);
      const results: AddResult[] = [];
      for (const p of paths) {
        const r = await invoke<AddResult>("add_file_with_progress", { filePath: p });
        results.push(r);
      }
      setUploads((prev) => [...prev, ...results]);
      setError("");
    } catch (e) {
      setError(`Upload failed: ${formatError(e)}`);
    } finally {
      setUploading(false);
      setUploadProgress(null);
    }
  }, [t]);

  // ── 开机自启 ──
  async function toggleAutoLaunch() {
    try {
      const newVal = !config?.auto_launch;
      await invoke("set_auto_launch", { enable: newVal });
      await loadConfig();
    } catch (e) {
      setError(`Auto-launch error: ${formatError(e)}`);
    }
  }

  const getStatusColor = () => {
    switch (status.type) {
      case "Running": return "#4caf50";
      case "Starting": case "Stopping": return "#ff9800";
      case "Failed": return "#f44336";
      default: return "#9e9e9e";
    }
  };

  const isRunning = status.type === "Running";

  return (
    <div className="container">
      <h1>{t("appTitle")}</h1>

      {/* ── Tab 导航 ── */}
      <nav className="tab-nav">
        {(["dashboard", "webui", "files", "pins", "ipns"] as TabName[]).map((tab) => (
          <button
            key={tab}
            className={`tab-btn ${activeTab === tab ? "active" : ""}`}
            onClick={() => { setActiveTab(tab); if (tab === "dashboard" && isRunning) { loadDashboard(); invoke("set_prefetch_hint", { hint: "dashboard" }); } if (tab === "pins" && isRunning) { loadPins(); invoke("set_prefetch_hint", { hint: "pins" }); } if (tab === "ipns" && isRunning) { loadKeyList(); invoke("set_prefetch_hint", { hint: "ipns" }); } }}
            disabled={tab === "webui" && !isRunning}
          >
            {t(tab)}
          </button>
        ))}
      </nav>

      {/* ═══════════════════════════════════════════════ */}
      {/* A3: 仪表盘 Dashboard                              */}
      {/* ═══════════════════════════════════════════════ */}
      {activeTab === "dashboard" && (
        <>
          {/* ── 状态卡片 ── */}
          <div className="status-card">
            <h2>{t("daemonStatus")}</h2>
            <div className="status-indicator" style={{ backgroundColor: getStatusColor() }}>
              {status.type}
            </div>
            {status.data && (
              <div className="status-details">
                {status.data.peer_id && <p>Peer ID: {status.data.peer_id}</p>}
                {status.data.pid !== undefined && status.data.pid > 0 && <p>PID: {status.data.pid}</p>}
                {status.data.api_addr && <p>API: {status.data.api_addr}</p>}
                {status.data.error && <p className="error">Error: {status.data.error}</p>}
              </div>
            )}
          </div>

          {/* ── 控制按钮 ── */}
          <div className="controls">
            <button onClick={startDaemon} disabled={status.type !== "Stopped" && status.type !== "Failed"}>
              {t("startDaemon")}
            </button>
            <button onClick={stopDaemon} disabled={status.type === "Stopped"}>
              {t("stopDaemon")}
            </button>
            <button onClick={restartDaemon}>{t("restartDaemon")}</button>
            <button onClick={loadStatus}>{t("refreshStatus")}</button>
            <button onClick={openWebui} disabled={!isRunning} className="btn-secondary">
              {t("openWebui")}
            </button>
          </div>

          {/* ── A3: 节点仪表盘 ── */}
          <div className="dashboard-section">
            <div className="section-header">
              <h2>{t("nodeDashboard")}</h2>
              <button onClick={loadDashboard} disabled={!isRunning || dashLoading} className="btn-small">
                {dashLoading ? "⏳" : "🔄"} {t("refresh")}
                {cacheHit && <span className="cache-indicator">{t("cacheIndicator")}</span>}
              </button>
            </div>

            {dashboard && (
              <div className="dashboard-grid">
                {/* 节点信息 */}
                <div className="dash-card">
                  <div className="dash-card-title">{t("nodeInfo")}</div>
                  {dashboard.node_id ? (
                    <>
                      <div className="dash-stat">
                        <span className="dash-label">Peer ID</span>
                        <span className="dash-value mono small">{dashboard.node_id.id.slice(0, 16)}…</span>
                      </div>
                      <div className="dash-stat">
                        <span className="dash-label">{t("agentVersion")}</span>
                        <span className="dash-value">{dashboard.node_id.agent_version}</span>
                      </div>
                    </>
                  ) : <div className="dash-na">N/A</div>}
                  {dashboard.version && (
                    <div className="dash-stat">
                      <span className="dash-label">{t("kuboVersion")}</span>
                      <span className="dash-value">{dashboard.version}</span>
                    </div>
                  )}
                </div>

                {/* 仓库统计 */}
                <div className="dash-card">
                  <div className="dash-card-title">{t("repoStats")}</div>
                  {dashboard.repo ? (
                    <>
                      <div className="dash-stat">
                        <span className="dash-label">{t("repoSize")}</span>
                        <span className="dash-value">{formatBytes(dashboard.repo.repo_size)}</span>
                      </div>
                      <div className="dash-stat">
                        <span className="dash-label">{t("numObjects")}</span>
                        <span className="dash-value">{dashboard.repo.num_objects.toLocaleString()}</span>
                      </div>
                    </>
                  ) : <div className="dash-na">N/A</div>}
                </div>

                {/* 网络连接 */}
                <div className="dash-card">
                  <div className="dash-card-title">{t("network")}</div>
                  {dashboard.peers ? (
                    <div className="dash-stat">
                      <span className="dash-label">{t("connectedPeers")}</span>
                      <span className="dash-value dash-big">{dashboard.peers.peers.length}</span>
                    </div>
                  ) : <div className="dash-na">N/A</div>}
                  {dashboard.bandwidth && (
                    <>
                      <div className="dash-stat">
                        <span className="dash-label">{t("rateIn")}</span>
                        <span className="dash-value green">{formatRate(dashboard.bandwidth.rate_in)}</span>
                      </div>
                      <div className="dash-stat">
                        <span className="dash-label">{t("rateOut")}</span>
                        <span className="dash-value orange">{formatRate(dashboard.bandwidth.rate_out)}</span>
                      </div>
                    </>
                  )}
                </div>

                {/* 数据交换 */}
                <div className="dash-card">
                  <div className="dash-card-title">{t("dataExchange")}</div>
                  {dashboard.bitswap ? (
                    <>
                      <div className="dash-stat">
                        <span className="dash-label">{t("dataReceived")}</span>
                        <span className="dash-value">{formatBytes(dashboard.bitswap.data_received)}</span>
                      </div>
                      <div className="dash-stat">
                        <span className="dash-label">{t("dataSent")}</span>
                        <span className="dash-value">{formatBytes(dashboard.bitswap.data_sent)}</span>
                      </div>
                      <div className="dash-stat">
                        <span className="dash-label">{t("blocksExchanged")}</span>
                        <span className="dash-value">
                          ↓{dashboard.bitswap.blocks_received} ↑{dashboard.bitswap.blocks_sent}
                        </span>
                      </div>
                    </>
                  ) : <div className="dash-na">N/A</div>}
                </div>

                {/* Pin 概览 */}
                <div className="dash-card">
                  <div className="dash-card-title">{t("pins")}</div>
                  <div className="dash-stat">
                    <span className="dash-label">{t("pinnedItems")}</span>
                    <span className="dash-value dash-big">{dashboard.pin_count}</span>
                  </div>
                  {dashboard.bandwidth && (
                    <>
                      <div className="dash-stat">
                        <span className="dash-label">{t("totalIn")}</span>
                        <span className="dash-value">{formatBytes(dashboard.bandwidth.total_in)}</span>
                      </div>
                      <div className="dash-stat">
                        <span className="dash-label">{t("totalOut")}</span>
                        <span className="dash-value">{formatBytes(dashboard.bandwidth.total_out)}</span>
                      </div>
                    </>
                  )}
                </div>
              </div>
            )}
            {!dashboard && isRunning && (
              <div className="dash-placeholder">{t("clickRefreshDashboard")}</div>
            )}
          </div>

          {/* ── Phase 4: 后端选择器 ── */}
          <div className="backend-bar">
            <span className="backend-label">🔗 {t("activeBackend")}:</span>
            <select className="select-input" value={activeBackend} onChange={async (e) => {
              const v = e.target.value;
              try { await invoke("switch_backend", { backendType: v }); setActiveBackend(v); }
              catch (e) { setError(formatError(e)); }
            }}>
              <option value="kubo">Kubo (Go)</option>
              {/* Iroh 后端目前仅为 stub（未实现文件/Pin/IPNS 等操作），暂不开放切换 */}
              <option value="iroh" disabled>Iroh (Rust) — 开发中 / not yet functional</option>
            </select>
            <button onClick={async () => {
              try {
                const caps = await invoke<Record<string,unknown>>("get_backend_capabilities");
                setBackendCaps(caps);
              } catch (e) { setError(formatError(e)); }
            }} className="btn-small">{t("viewCapabilities")}</button>
            {backendCaps && (
              <span className="backend-caps-badge" title={JSON.stringify(backendCaps, null, 2)}>
                IPNS:{(backendCaps as any).ipns ? "✅" : "❌"} Pin:{(backendCaps as any).pinning ? "✅" : "❌"}
              </span>
            )}
            <button onClick={async () => {
              try {
                setBenchRunning(true);
                const result = await invoke<Record<string,unknown>>("run_benchmark");
                setBenchResult(result);
              } catch (e) { setError(formatError(e)); }
              finally { setBenchRunning(false); }
            }} className="btn-small btn-download" disabled={benchRunning}>
              {benchRunning ? "⏳" : "⚡"} {t("runBenchmark")}
            </button>
            <button onClick={async () => {
              try {
                setCompatRunning(true);
                const result = await invoke<Record<string,unknown>>("run_compat_test");
                setCompatResult(result);
              } catch (e) { setError(formatError(e)); }
              finally { setCompatRunning(false); }
            }} className="btn-small btn-pin" disabled={compatRunning}>
              {compatRunning ? "⏳" : "🧪"} {t("runCompatTest")}
            </button>
          </div>

          {/* ── Phase 3: 代理统计 + 离线队列 + 带宽控制 ── */}
          {isRunning && (
            <>
            <div className="phase3-grid">

              {/* 代理统计 */}
              <div className="phase3-card">
                <div className="dash-card-title">⚡ {t("proxyStats")}</div>
                <div className="phase3-row">
                  <span>{t("cacheHitRate")}</span>
                  <span className="dash-value">
                    {proxyStats && proxyStats.total_requests > 0
                      ? `${Math.round(proxyStats.cache_hits/proxyStats.total_requests*100)}%`
                      : "—"}
                  </span>
                </div>
                <div className="phase3-row">
                  <span>{t("avgLatency")}</span>
                  <span className="dash-value">{proxyStats ? `${proxyStats.avg_latency_ms.toFixed(1)} ms` : "—"}</span>
                </div>
                <div className="phase3-row">
                  <span>{t("circuitBreaker")}</span>
                  <span className={`dash-value ${proxyStats && proxyStats.circuit_open_count > 0 ? "red" : "green"}`}>
                    {proxyStats && proxyStats.circuit_open_count > 0 ? `⚠️ ${proxyStats.circuit_open_count}x` : "✅ OK"}
                  </span>
                </div>
              </div>

              {/* 离线队列 */}
              <div className="phase3-card">
                <div className="dash-card-title">📋 {t("offlineQueue")}</div>
                <div className="phase3-row">
                  <span>{t("pendingOps")}</span>
                  <span className={`dash-value ${offlineCount > 0 ? "orange" : "green"}`}>
                    {offlineCount}
                  </span>
                </div>
                <button onClick={async () => {
                  try {
                    const r = await invoke<{count:number}>("get_offline_queue");
                    setOfflineCount(r.count);
                    if (r.count > 0) await invoke("flush_offline_queue");
                  } catch (e) { setError(formatError(e)); }
                }} disabled={offlineCount === 0} className="btn-small btn-pin"
                  style={{marginTop:8,width:"100%"}}>
                  {offlineCount > 0 ? `▶ ${t("flushQueue")} (${offlineCount})` : t("queueEmpty")}
                </button>
              </div>

              {/* 带宽控制 */}
              <div className="phase3-card">
                <div className="dash-card-title">📶 {t("bandwidthControl")}</div>
                <div className="phase3-row">
                  <span>{t("rateIn")}</span>
                  <span className="dash-value green">{bwStatus ? formatRate(bwStatus.rate_in) : "—"}</span>
                </div>
                <div className="phase3-row">
                  <span>{t("rateOut")}</span>
                  <span className="dash-value orange">{bwStatus ? formatRate(bwStatus.rate_out) : "—"}</span>
                </div>
                <div className="phase3-row">
                  <span>{t("maxConnections")}</span>
                  <input type="range" min="50" max="2000" step="50" value={bwConfig.max_connections}
                    onChange={async (e) => {
                      const v = parseInt(e.target.value);
                      const newCfg = {...bwConfig, max_connections: v};
                      setBwConfig(newCfg);
                      try { await invoke("set_bandwidth_config", { config: newCfg }); }
                      catch (e) { setError(formatError(e)); }
                    }} className="phase3-slider" />
                  <span className="dash-value small">{bwConfig.max_connections}</span>
                </div>
              </div>
            </div>

            {/* 基准测试结果 */}
            {benchResult && (
              <div className="bench-result">
                <h3>⚡ {t("benchmarkResults")}</h3>
                <div className="bench-summary">
                  <span>{t("winner")}: {(benchResult as any).winner || "—"}</span>
                  <span>{t("speedup")}: {(benchResult as any).speedup_ratio ? `${((benchResult as any).speedup_ratio).toFixed(2)}x` : "—"}</span>
                  <span>{t("duration")}: {(benchResult as any).total_duration_ms}ms</span>
                </div>
                {((benchResult as any).operations as any[])?.map((op: any, i: number) => (
                  <div key={i} className="bench-row">
                    <span>{op.operation}</span>
                    <span>{op.backend}</span>
                    <span>{op.avg_ms?.toFixed(1)}ms avg</span>
                    <span>{op.throughput_ops?.toFixed(0)} ops/s</span>
                  </div>
                ))}
              </div>
            )}

            {/* 兼容性测试结果 */}
            {compatResult && (
              <div className="compat-result">
                <h3>🧪 {t("compatResults")}</h3>
                <div className="compat-score">
                  {(compatResult as any).compatibility_score?.toFixed(0)}% {t("compatible")}
                </div>
                <div className="compat-summary">
                  <span>✅ {(compatResult as any).passed}</span>
                  <span>❌ {(compatResult as any).failed}</span>
                  <span>⏭ {(compatResult as any).skipped}</span>
                </div>
              </div>
            )}
            </>
          )}

          {/* ── 配置信息 ── */}
          {config && (
            <div className="config-card">
              <h2>{t("configuration")}</h2>
              <div className="config-item"><strong>{t("ipfsPath")}:</strong> {config.ipfs_path ?? t("default")}</div>
              <div className="config-item"><strong>{t("apiAddress")}:</strong> {config.api_addr}</div>
              <div className="config-item"><strong>{t("gateway")}:</strong> {config.gateway_addr}</div>
              <div className="config-item"><strong>{t("daemonFlags")}:</strong> {config.daemon_flags.join(", ") || t("none")}</div>
              <div className="config-item config-toggle" onClick={toggleAutoLaunch}>
                <strong>{t("autoLaunch")}:</strong> {config.auto_launch ? t("yes") : t("no")}
                <span className="toggle-hint">(click to toggle)</span>
              </div>
              <div className="config-item"><strong>{t("autoGC")}:</strong> {config.auto_gc ? t("yes") : t("no")}</div>
            </div>
          )}
        </>
      )}

      {/* ═══════════════════════════════════════════════ */}
      {/* WebUI iframe                                     */}
      {/* ═══════════════════════════════════════════════ */}
      {activeTab === "webui" && webuiUrl && (
        <div className="webui-container">
          <div className="webui-toolbar">
            <span>IPFS WebUI</span>
            <button onClick={openWebui} className="btn-small">{t("openBrowser")}</button>
          </div>
          <iframe src={webuiUrl} className="webui-iframe" title="IPFS WebUI" sandbox="allow-scripts allow-same-origin allow-forms" />
        </div>
      )}

      {/* ═══════════════════════════════════════════════ */}
      {/* A1: 文件管理（上传 + 下载）                      */}
      {/* ═══════════════════════════════════════════════ */}
      {activeTab === "files" && (
        <div className="files-section">
          {/* ── 上传 ── */}
          <h2>{t("uploadFiles")}</h2>
          <div className="drop-zone" onClick={selectAndUpload}>
            <p>{t("dropHere")}</p>
            <button className="btn-secondary" disabled={uploading || !isRunning}>
              {uploading ? t("uploading") + "..." : t("selectFiles")}
            </button>
          </div>

          {/* 上传进度条 */}
          {uploadProgress && (
            <div className="progress-bar-container">
              <div className="progress-label">
                {uploadProgress.name} — {formatBytes(uploadProgress.loaded)} / {formatBytes(uploadProgress.total)}
              </div>
              <div className="progress-bar">
                <div
                  className="progress-fill"
                  style={{ width: `${uploadProgress.total > 0 ? (uploadProgress.loaded / uploadProgress.total) * 100 : 0}%` }}
                />
              </div>
            </div>
          )}

          {/* ── A1 下载 ── */}
          <h2>{t("downloadFiles")}</h2>
          <div className="download-section">
            <div className="input-row">
              <input
                type="text"
                className="cid-input"
                placeholder={t("enterCid")}
                value={downloadCid}
                onChange={(e) => setDownloadCid(e.target.value)}
                disabled={!isRunning}
              />
              <button onClick={catByCid} disabled={!isRunning || !downloadCid.trim()} className="btn-small">
                🔍 {t("preview")}
              </button>
              <button onClick={downloadByCid} disabled={!isRunning || !downloadCid.trim() || downloading} className="btn-small btn-download">
                {downloading ? "⏳" : "⬇"} {t("download")}
              </button>
            </div>
          </div>

          {/* 下载进度条 */}
          {downloadProgress && (
            <div className="progress-bar-container">
              <div className="progress-label">
                {downloadProgress.cid.slice(0, 20)}… — {formatBytes(downloadProgress.loaded)}
                {downloadProgress.total ? ` / ${formatBytes(downloadProgress.total)}` : ""}
              </div>
              <div className="progress-bar">
                <div
                  className="progress-fill download-fill"
                  style={{ width: `${downloadProgress.total ? (downloadProgress.loaded / downloadProgress.total) * 100 : 50}%` }}
                />
              </div>
            </div>
          )}

          {/* 预览内容 */}
          {catResult && (
            <div className="preview-box">
              <h3>{t("preview")}</h3>
              <pre>{catResult}{catResult.length >= 5000 ? "\n…(truncated)" : ""}</pre>
            </div>
          )}

          {/* 已上传列表 */}
          {uploads.length > 0 && (
            <div className="uploads-list">
              <h3>{t("uploadedFiles")}</h3>
              <table>
                <thead>
                  <tr><th>{t("name")}</th><th>{t("hash")}</th><th>{t("size")}</th></tr>
                </thead>
                <tbody>
                  {uploads.map((f, i) => (
                    <tr key={i}>
                      <td>{f.Name}</td>
                      <td className="hash-cell">{f.Hash}</td>
                      <td>{f.Size}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}

      {/* ═══════════════════════════════════════════════ */}
      {/* A2: Pin 管理面板                                  */}
      {/* ═══════════════════════════════════════════════ */}
      {activeTab === "pins" && (
        <div className="pins-section">
          <div className="section-header">
            <h2>{t("pinManagement")}</h2>
            <button onClick={loadPins} disabled={!isRunning || pinLoading} className="btn-small">
              {pinLoading ? "⏳" : "🔄"} {t("refresh")}
            </button>
          </div>

          {/* 添加 Pin */}
          <div className="input-row">
            <input
              type="text"
              className="cid-input"
              placeholder={t("enterCidToPin")}
              value={pinCid}
              onChange={(e) => setPinCid(e.target.value)}
              disabled={!isRunning}
            />
            <button onClick={addPinByCid} disabled={!isRunning || !pinCid.trim()} className="btn-small btn-pin">
              📌 {t("pin")}
            </button>
          </div>

          {/* Pin 列表 */}
          {pinList.length > 0 && (
            <div className="pin-table-container">
              <table className="pin-table">
                <thead>
                  <tr>
                    <th>CID</th>
                    <th>{t("type")}</th>
                    <th>{t("actions")}</th>
                  </tr>
                </thead>
                <tbody>
                  {pinList.map((pin, i) => (
                    <tr key={i}>
                      <td className="hash-cell" title={pin.Cid}>{pin.Cid}</td>
                      <td><span className={`pin-type-badge ${pin.Type}`}>{pin.Type}</span></td>
                      <td>
                        <button
                          onClick={() => removePinByCid(pin.Cid)}
                          className="btn-small btn-danger"
                          disabled={!isRunning}
                        >
                          ❌ {t("unpin")}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}

          {pinList.length === 0 && !pinLoading && isRunning && (
            <div className="empty-state">{t("noPinnedItems")}</div>
          )}
          {!isRunning && (
            <div className="empty-state">{t("startDaemonToManagePins")}</div>
          )}
        </div>
      )}

      {/* ═══════════════════════════════════════════════ */}
      {/* Phase 2: IPNS 发布/解析 + 密钥管理                */}
      {/* ═══════════════════════════════════════════════ */}
      {activeTab === "ipns" && (
        <div className="ipns-section">
          <div className="section-header">
            <h2>{t("ipnsManagement")}</h2>
            <button onClick={loadKeyList} disabled={!isRunning} className="btn-small">{t("refresh")}</button>
          </div>

          {/* ── IPNS 发布 ── */}
          <div className="ipns-card">
            <h3>📤 {t("ipnsPublish")}</h3>
            <div className="input-row">
              <input type="text" className="cid-input" placeholder={t("enterCidToPublish")}
                value={ipnsCid} onChange={(e) => setIpnsCid(e.target.value)} disabled={!isRunning} />
            </div>
            <div className="input-row">
              <input type="text" className="cid-input small-input" placeholder={t("keyName")}
                value={ipnsKeyName} onChange={(e) => setIpnsKeyName(e.target.value)} disabled={!isRunning} />
              <select className="select-input" value={ipnsLifetime}
                onChange={(e) => setIpnsLifetime(e.target.value)} disabled={!isRunning}>
                <option value="24h">24h</option>
                <option value="48h">48h</option>
                <option value="72h">72h</option>
                <option value="168h">7d</option>
              </select>
              <button onClick={publishIpns} disabled={!isRunning || !ipnsCid.trim()} className="btn-small btn-pin">
                📤 {t("publish")}
              </button>
            </div>
            {ipnsPublishResult && (
              <div className="ipns-result success">{ipnsPublishResult}</div>
            )}
          </div>

          {/* ── IPNS 解析 ── */}
          <div className="ipns-card">
            <h3>📥 {t("ipnsResolve")}</h3>
            <div className="input-row">
              <input type="text" className="cid-input" placeholder={t("enterIpnsName")}
                value={ipnsResolveName} onChange={(e) => setIpnsResolveName(e.target.value)} disabled={!isRunning} />
              <button onClick={resolveIpns} disabled={!isRunning || !ipnsResolveName.trim()} className="btn-small btn-download">
                🔍 {t("resolve")}
              </button>
            </div>
            {ipnsResolveResult && (
              <div className="ipns-result">{ipnsResolveResult}</div>
            )}
          </div>

          {/* ── 密钥管理 ── */}
          <div className="ipns-card">
            <h3>🔑 {t("keyManagement")}</h3>
            <div className="input-row">
              <input type="text" className="cid-input small-input" placeholder={t("newKeyLabel")}
                value={newKeyLabel} onChange={(e) => setNewKeyLabel(e.target.value)} disabled={!isRunning} />
              <button onClick={generateNewKey} disabled={!isRunning || !newKeyLabel.trim()} className="btn-small btn-download">
                + {t("generateKey")}
              </button>
            </div>
            {keyList.length > 0 && (
              <div className="key-table-container">
                <table className="pin-table">
                  <thead>
                    <tr><th>{t("label")}</th><th>IPNS Name</th><th>{t("actions")}</th></tr>
                  </thead>
                  <tbody>
                    {keyList.map((k, i) => (
                      <tr key={i}>
                        <td className="key-label-cell">{k.label}</td>
                        <td className="hash-cell" title={k.ipns_name}>{k.ipns_name}</td>
                        <td>
                          <button onClick={() => deleteKeyByLabel(k.label)} className="btn-small btn-danger"
                            disabled={!isRunning}>❌</button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            {keyList.length === 0 && isRunning && (
              <div className="empty-state">{t("noKeysGenerated")}</div>
            )}
          </div>
        </div>
      )}

      {/* ── 全局错误 ── */}
      {error && <div className="error-message">{error}</div>}

      <footer><p>{t("builtWith")}</p></footer>
    </div>
  );
}

export default App;
