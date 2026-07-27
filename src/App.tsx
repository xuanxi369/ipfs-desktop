import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import "./App.css";

import {
  DaemonStatus, AppConfig, AddResult, PinEntry, PinList,
  DashboardStats, DownloadProgress, UploadProgress,
  NodeIdentityInfo, NodeHealth, DashboardTick,
  formatError, formatBytes, TabName,
} from "./types";

import Dashboard from "./Dashboard";
import WebUI from "./WebUI";
import Files from "./Files";
import PinManager from "./PinManager";
import IpnsManager from "./IpnsManager";
import IrohNative from "./IrohNative";

// ── 主组件 ──

function App() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<DaemonStatus>({ type: "Stopped" });
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [error, setError] = useState<string>("");
  const [activeTab, setActiveTab] = useState<TabName>("dashboard");
  const [uploads, setUploads] = useState<AddResult[]>([]);
  const [uploading, setUploading] = useState(false);

  // ── Phase B/C: iroh 原生收发 + 路由状态 ──
  const [irohInfo, setIrohInfo] = useState<{ peer_id: string; agent_version: string } | null>(null);
  const [irohCid, setIrohCid] = useState("");
  const [irohTicket, setIrohTicket] = useState("");
  const [irohFetchInput, setIrohFetchInput] = useState("");
  const [irohFetchResult, setIrohFetchResult] = useState<string>("");
  const [irohBusy, setIrohBusy] = useState(false);
  const [routePolicy, setRoutePolicy] = useState<string>("KuboOnly");

  // ── Phase D1: 节点身份 ──
  const [identity, setIdentity] = useState<NodeIdentityInfo | null>(null);
  const [editingLabel, setEditingLabel] = useState(false);
  const [labelDraft, setLabelDraft] = useState("");

  // ── Phase D3: 节点健康度 ──
  const [health, setHealth] = useState<NodeHealth | null>(null);

  // ── A1 下载状态 ──
  const [downloadCid, setDownloadCid] = useState("");
  const [downloadProgress, setDownloadProgress] = useState<DownloadProgress | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [catResult, setCatResult] = useState<string>("");
  const [routeHint, setRouteHint] = useState<string>("");

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
  const [keyList, setKeyList] = useState<{ public_key: string; ipns_name: string; label: string }[]>([]);
  const [newKeyLabel, setNewKeyLabel] = useState("");

  // ── 缓存指示器 ──
  const [cacheHit, setCacheHit] = useState(false);

  // ── Phase 3: 代理统计 ──
  const [proxyStats, setProxyStats] = useState<{ total_requests: number; cache_hits: number; api_calls: number; circuit_open_count: number; avg_latency_ms: number } | null>(null);

  // ── Phase 3: 离线队列 ──
  const [offlineCount, setOfflineCount] = useState(0);

  // ── Phase 3: 带宽控制 ──
  const [bwConfig, setBwConfig] = useState<{ max_connections: number; max_streams: number; upload_limit: number; download_limit: number; enabled: boolean }>({ max_connections: 600, max_streams: 2048, upload_limit: 0, download_limit: 0, enabled: true });
  const [bwStatus, setBwStatus] = useState<{ rate_in: number; rate_out: number; total_in: number; total_out: number } | null>(null);

  // ── Phase 4: 后端切换 ──
  const [activeBackend, setActiveBackend] = useState("kubo");
  const [backendCaps, setBackendCaps] = useState<Record<string, unknown> | null>(null);
  const [benchResult, setBenchResult] = useState<Record<string, unknown> | null>(null);
  const [benchRunning, setBenchRunning] = useState(false);
  const [compatResult, setCompatResult] = useState<Record<string, unknown> | null>(null);
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
    const unlistenReplay = listen<{ success: number; failed: number; remaining: number }>("replay-progress", (event) => {
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
        const ps = await invoke<{ total_requests: number; cache_hits: number; api_calls: number; circuit_open_count: number; avg_latency_ms: number }>("get_proxy_stats");
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
      const result = await invoke<{ Name: string; Value: string }>("ipns_publish", {
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
      const result = await invoke<{ Path: string }>("ipns_resolve", { name: ipnsResolveName.trim() });
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
      const keys = await invoke<{ public_key: string; ipns_name: string; label: string }[]>("list_keys");
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

  const isRunning = status.type === "Running";

  // ── Phase C: 输入 CID 时显示它在当前策略下会路由到哪个后端（debounce）──
  useEffect(() => {
    const cid = downloadCid.trim();
    if (!cid) { setRouteHint(""); return; }
    const h = setTimeout(async () => {
      try { setRouteHint(await invoke<string>("get_backend_route", { cid })); }
      catch { setRouteHint(""); }
    }, 300);
    return () => clearTimeout(h);
  }, [downloadCid]);

  // ── Phase D1: 节点身份处理 ──
  async function loadIdentity() {
    try { setIdentity(await invoke<NodeIdentityInfo>("get_node_identity")); }
    catch (e) { setError(`identity: ${formatError(e)}`); }
  }
  async function saveLabel() {
    try {
      const info = await invoke<NodeIdentityInfo>("set_node_label", { label: labelDraft.trim() });
      setIdentity(info);
      setEditingLabel(false);
      setError("");
    } catch (e) { setError(`identity: ${formatError(e)}`); }
  }
  async function exportIdentity() {
    try {
      const doc = await invoke<string>("export_identity");
      await navigator.clipboard?.writeText(doc);
      setError("");
    } catch (e) { setError(`identity: ${formatError(e)}`); }
  }

  async function loadHealth() {
    try { setHealth(await invoke<NodeHealth>("get_node_health")); }
    catch (e) { setError(`health: ${formatError(e)}`); }
  }

  // ── Phase B/C: iroh 原生收发 + 路由处理 ──
  async function loadIrohInfo() {
    try {
      const info = await invoke<{ peer_id: string; agent_version: string }>("iroh_node_info");
      setIrohInfo(info);
      setError("");
    } catch (e) {
      setIrohInfo(null);
      setError(`iroh: ${formatError(e)}`);
    }
  }

  async function loadRoutePolicy() {
    try {
      const p = await invoke<string>("get_route_policy");
      setRoutePolicy(p);
    } catch { /* ignore */ }
  }

  async function irohAddFile() {
    try {
      const selected = await open({ multiple: false, title: t("irohAddFile") });
      if (!selected || typeof selected !== "string") return;
      setIrohBusy(true);
      const out = await invoke<{ cid: string; size: number; name: string }>("iroh_add_file", { filePath: selected });
      setIrohCid(out.cid);
      setIrohTicket("");
      setError("");
    } catch (e) {
      setError(`iroh add: ${formatError(e)}`);
    } finally {
      setIrohBusy(false);
    }
  }

  async function irohShare() {
    if (!irohCid.trim()) return;
    try {
      setIrohBusy(true);
      const ticket = await invoke<string>("iroh_share", { cid: irohCid.trim() });
      setIrohTicket(ticket);
      setError("");
    } catch (e) {
      setError(`iroh share: ${formatError(e)}`);
    } finally {
      setIrohBusy(false);
    }
  }

  async function irohFetch() {
    if (!irohFetchInput.trim()) return;
    try {
      setIrohBusy(true);
      const savePath = await save({ title: t("saveDownloadAs") });
      const res = await invoke<{ size: number; saved: string | null }>("iroh_fetch_ticket", {
        ticket: irohFetchInput.trim(),
        savePath: savePath || null,
      });
      setIrohFetchResult(
        `${formatBytes(res.size)} — ${res.saved ? `${t("saved")}: ${res.saved}` : t("notSaved")}`
      );
      setError("");
    } catch (e) {
      setError(`iroh fetch: ${formatError(e)}`);
    } finally {
      setIrohBusy(false);
    }
  }

  return (
    <div className="container">
      <h1>{t("appTitle")}</h1>

      {/* ── Tab 导航 ── */}
      <nav className="tab-nav">
        {(["dashboard", "webui", "files", "pins", "ipns", "iroh"] as TabName[]).map((tab) => (
          <button
            key={tab}
            className={`tab-btn ${activeTab === tab ? "active" : ""}`}
            onClick={() => { setActiveTab(tab); if (tab === "dashboard") { loadIdentity(); loadHealth(); } if (tab === "dashboard" && isRunning) { loadDashboard(); invoke("set_prefetch_hint", { hint: "dashboard" }); } if (tab === "pins" && isRunning) { loadPins(); invoke("set_prefetch_hint", { hint: "pins" }); } if (tab === "ipns" && isRunning) { loadKeyList(); invoke("set_prefetch_hint", { hint: "ipns" }); } if (tab === "iroh") { loadIrohInfo(); loadRoutePolicy(); } }}
            disabled={tab === "webui" && !isRunning}
          >
            {t(tab)}
          </button>
        ))}
      </nav>

      {/* ═══════════════════════════════════════════════ */}
      {/* Dashboard 标签                                   */}
      {/* ═══════════════════════════════════════════════ */}
      {activeTab === "dashboard" && (
        <Dashboard
          status={status}
          config={config}
          isRunning={isRunning}
          dashboard={dashboard}
          dashLoading={dashLoading}
          cacheHit={cacheHit}
          identity={identity}
          editingLabel={editingLabel}
          labelDraft={labelDraft}
          health={health}
          activeBackend={activeBackend}
          backendCaps={backendCaps}
          benchResult={benchResult}
          benchRunning={benchRunning}
          compatResult={compatResult}
          compatRunning={compatRunning}
          proxyStats={proxyStats}
          offlineCount={offlineCount}
          bwConfig={bwConfig}
          bwStatus={bwStatus}
          setError={setError}
          setActiveBackend={setActiveBackend}
          setBackendCaps={setBackendCaps}
          setBenchResult={setBenchResult}
          setBenchRunning={setBenchRunning}
          setCompatResult={setCompatResult}
          setCompatRunning={setCompatRunning}
          setOfflineCount={setOfflineCount}
          setBwConfig={setBwConfig}
          setEditingLabel={setEditingLabel}
          setLabelDraft={setLabelDraft}
          loadDashboard={loadDashboard}
          loadIdentity={loadIdentity}
          loadHealth={loadHealth}
          saveLabel={saveLabel}
          exportIdentity={exportIdentity}
          startDaemon={startDaemon}
          stopDaemon={stopDaemon}
          restartDaemon={restartDaemon}
          loadStatus={loadStatus}
          openWebui={openWebui}
          toggleAutoLaunch={toggleAutoLaunch}
        />
      )}

      {/* ═══════════════════════════════════════════════ */}
      {/* WebUI 标签                                      */}
      {/* ═══════════════════════════════════════════════ */}
      {activeTab === "webui" && webuiUrl && (
        <WebUI webuiUrl={webuiUrl} openWebui={openWebui} />
      )}

      {/* ═══════════════════════════════════════════════ */}
      {/* Files 标签                                      */}
      {/* ═══════════════════════════════════════════════ */}
      {activeTab === "files" && (
        <Files
          isRunning={isRunning}
          uploading={uploading}
          downloadCid={downloadCid}
          downloadProgress={downloadProgress}
          downloading={downloading}
          catResult={catResult}
          uploadProgress={uploadProgress}
          uploads={uploads}
          routeHint={routeHint}
          setDownloadCid={setDownloadCid}
          selectAndUpload={selectAndUpload}
          catByCid={catByCid}
          downloadByCid={downloadByCid}
        />
      )}

      {/* ═══════════════════════════════════════════════ */}
      {/* Pins 标签                                       */}
      {/* ═══════════════════════════════════════════════ */}
      {activeTab === "pins" && (
        <PinManager
          isRunning={isRunning}
          pinList={pinList}
          pinLoading={pinLoading}
          pinCid={pinCid}
          setPinCid={setPinCid}
          loadPins={loadPins}
          addPinByCid={addPinByCid}
          removePinByCid={removePinByCid}
        />
      )}

      {/* ═══════════════════════════════════════════════ */}
      {/* IPNS 标签                                       */}
      {/* ═══════════════════════════════════════════════ */}
      {activeTab === "ipns" && (
        <IpnsManager
          isRunning={isRunning}
          ipnsCid={ipnsCid}
          ipnsKeyName={ipnsKeyName}
          ipnsLifetime={ipnsLifetime}
          ipnsResolveName={ipnsResolveName}
          ipnsResolveResult={ipnsResolveResult}
          ipnsPublishResult={ipnsPublishResult}
          keyList={keyList}
          newKeyLabel={newKeyLabel}
          setIpnsCid={setIpnsCid}
          setIpnsKeyName={setIpnsKeyName}
          setIpnsLifetime={setIpnsLifetime}
          setIpnsResolveName={setIpnsResolveName}
          setNewKeyLabel={setNewKeyLabel}
          publishIpns={publishIpns}
          resolveIpns={resolveIpns}
          generateNewKey={generateNewKey}
          loadKeyList={loadKeyList}
          deleteKeyByLabel={deleteKeyByLabel}
        />
      )}

      {/* ═══════════════════════════════════════════════ */}
      {/* iroh 原生标签                                    */}
      {/* ═══════════════════════════════════════════════ */}
      {activeTab === "iroh" && (
        <IrohNative
          irohInfo={irohInfo}
          irohCid={irohCid}
          irohTicket={irohTicket}
          irohFetchInput={irohFetchInput}
          irohFetchResult={irohFetchResult}
          irohBusy={irohBusy}
          routePolicy={routePolicy}
          setError={setError}
          setIrohFetchInput={setIrohFetchInput}
          setRoutePolicy={setRoutePolicy}
          loadIrohInfo={loadIrohInfo}
          irohAddFile={irohAddFile}
          irohShare={irohShare}
          irohFetch={irohFetch}
        />
      )}

      {/* ── 全局错误 ── */}
      {error && <div className="error-message">{error}</div>}

      <footer><p>{t("builtWith")}</p></footer>
    </div>
  );
}

export default App;
