import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import "./App.css";

import {
  DashboardStats, DownloadProgress, UploadProgress,
  NodeIdentityInfo, NodeHealth, DashboardTick,
  formatError, TabName,
} from "./types";

import Dashboard from "./Dashboard";
import WebUI from "./WebUI";
import Files from "./Files";
import PinManager from "./PinManager";
import IpnsManager from "./IpnsManager";
import IrohNative from "./IrohNative";
import { Icon } from "./Icons";
import AdvancedTools from "./AdvancedTools";
import { useTheme } from "./hooks/useTheme";
import { useDaemon } from "./hooks/useDaemon";
import { useContent } from "./hooks/useContent";
import { useIpns } from "./hooks/useIpns";
import { useIroh } from "./hooks/useIroh";
import { mergeDashboardTick } from "./dashboardTick";

// ── 主组件 ──

function App() {
  const { t, i18n } = useTranslation();
  const [error, setError] = useState<string>("");
  const [activeTab, setActiveTab] = useState<TabName>("dashboard");
  const { theme, setTheme } = useTheme();
  const daemon = useDaemon(setError);
  const content = useContent(setError, t);
  const ipns = useIpns(setError);
  const iroh = useIroh(setError, t);
  const { status, config, setConfig, loadStatus, startDaemon, stopDaemon, restartDaemon,
    openWebui, toggleAutoLaunch } = daemon;
  const { uploads, contentRecords, uploading, uploadProgress, setUploadProgress, downloadCid,
    setDownloadCid, downloadProgress, setDownloadProgress, downloading, catResult, routeHint,
    pinList, pinLoading, pinCid, setPinCid, loadContentRecords, removeContentRecord,
    selectAndUpload, downloadByCid, catByCid, loadPins, addPinByCid, removePinByCid } = content;
  const { ipnsCid, ipnsKeyName, ipnsLifetime, ipnsResolveName, ipnsResolveResult, ipnsPublishResult,
    keyList, newKeyLabel, setIpnsCid, setIpnsKeyName, setIpnsLifetime, setIpnsResolveName,
    setNewKeyLabel, publishIpns, resolveIpns, generateNewKey, loadKeyList, deleteKeyByLabel } = ipns;
  const { irohInfo, irohCid, irohTicket, irohFetchInput, irohFetchResult, irohBusy, routePolicy, migrationStatus,
    setIrohFetchInput, setRoutePolicy, loadIrohInfo, loadRoutePolicy, irohAddFile, irohShare,
    irohFetch, irohKeep, irohShutdown, irohUnkeep, irohRegisterTicket } = iroh;

  // ── Phase B/C: iroh 原生收发 + 路由状态 ──

  // ── Phase D1: 节点身份 ──
  const [identity, setIdentity] = useState<NodeIdentityInfo | null>(null);
  const [editingLabel, setEditingLabel] = useState(false);
  const [labelDraft, setLabelDraft] = useState("");

  // ── Phase D3: 节点健康度 ──
  const [health, setHealth] = useState<NodeHealth | null>(null);

  // ── A1 下载状态 ──

  // ── A2 Pin 状态 ──

  // ── A3 仪表盘状态 ──
  const [dashboard, setDashboard] = useState<DashboardStats | null>(null);
  const [dashLoading, setDashLoading] = useState(false);

  // ── 上传进度 ──

  // ── Phase 2: IPNS 状态 ──

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
  const [backendCaps, setBackendCaps] = useState<Record<string, unknown> | null>(null);
  const [benchResult, setBenchResult] = useState<Record<string, unknown> | null>(null);
  const [benchRunning, setBenchRunning] = useState(false);
  const [compatResult, setCompatResult] = useState<Record<string, unknown> | null>(null);
  const [compatRunning, setCompatRunning] = useState(false);

  const webuiUrl = config ? `${config.api_addr}/webui` : "";

  // ── 初始化 ──
  useEffect(() => {
    let mounted = true;
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
      setDashboard((previous) => mergeDashboardTick(previous, tick));
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
    void loadContentRecords();
    return () => {
      mounted = false;
      unlistenDownload.then((fn) => fn());
      unlistenUpload.then((fn) => fn());
      unlistenDash.then((fn) => fn());
      unlistenReplay.then((fn) => fn());
    };
  }, []);

  // ── 数据加载 ──
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
  const isRunning = status.type === "Running";

  // ── Phase C: 输入 CID 时显示它在当前策略下会路由到哪个后端（debounce）──
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
  const navItems: { tab: TabName; icon: string; group: string }[] = [
    { tab: "dashboard", icon: "dashboard", group: "Node" },
    { tab: "files", icon: "files", group: "Content" },
    { tab: "pins", icon: "pins", group: "Content" },
    { tab: "ipns", icon: "ipns", group: "Publishing" },
    { tab: "iroh", icon: "iroh", group: "Network" },
    { tab: "webui", icon: "web", group: "Advanced" },
    { tab: "advanced", icon: "flask", group: "Advanced" },
  ];
  const pageTitle = t(activeTab);
  const navigate = (tab: TabName) => {
    setActiveTab(tab);
    if (tab === "dashboard") { loadIdentity(); loadHealth(); }
    if (tab === "dashboard" && isRunning) { loadDashboard(); invoke("set_prefetch_hint", { hint: "dashboard" }); }
    if (tab === "pins" && isRunning) { loadPins(); invoke("set_prefetch_hint", { hint: "pins" }); }
    if (tab === "ipns" && isRunning) { loadKeyList(); invoke("set_prefetch_hint", { hint: "ipns" }); }
    if (tab === "iroh") { loadIrohInfo(); loadRoutePolicy(); }
  };

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand"><div className="brand-mark"><Icon name="cube"/></div><div><strong>IPFS</strong><span>Content Network</span></div></div>
        <nav className="side-nav">
          {navItems.map(({ tab, icon, group }, index) => (
            <div key={tab}>
              {(index === 0 || navItems[index - 1].group !== group) && <div className="nav-group">{group}</div>}
          <button
            className={`nav-item ${activeTab === tab ? "active" : ""}`}
            onClick={() => navigate(tab)}
          ><Icon name={icon}/><span>{t(tab)}</span>{tab === "iroh" && <em>LAB</em>}</button>
            </div>
        ))}
        </nav>
        <div className="sidebar-status"><span className={`status-dot ${isRunning ? "online" : ""}`}/><div><strong>{isRunning ? t("nodeOnline") : t("nodeOffline")}</strong><small>{routePolicy === "Compatible" ? t("smartRouting") : routePolicy}</small></div></div>
      </aside>
      <section className="workspace">
        <header className="topbar">
          <div className="topbar-copy"><p className="eyebrow">IPFS DESKTOP</p><h1>{pageTitle}</h1></div>
          <div className="topbar-actions">
            <span className={`node-pill ${isRunning ? "online" : ""}`}><span className="status-dot"/>{status.type}</span>
            <span className="route-pill">{routePolicy === "Compatible" ? t("smartRouting") : routePolicy}</span>
            <button className="icon-button" onClick={() => setTheme(theme === "dark" ? "light" : "dark")} title={t("toggleTheme")}><Icon name={theme === "dark" ? "sun" : "moon"}/></button>
            <button className="icon-button" onClick={() => i18n.changeLanguage(i18n.language === "zh" ? "en" : "zh")} title={i18n.language === "zh" ? "Switch to English" : "切换到中文"} style={{fontSize:"11px",fontWeight:700,width:"auto",padding:"0 10px"}}>{i18n.language === "zh" ? "EN" : "中"}</button>
          </div>
        </header>
        <main className="main-content">

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
          contentRecords={contentRecords}
          loadContentRecords={loadContentRecords}
          routeHint={routeHint}
          setDownloadCid={setDownloadCid}
          selectAndUpload={selectAndUpload}
          catByCid={catByCid}
          downloadByCid={downloadByCid}
          removeContentRecord={removeContentRecord}
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
          migrationStatus={migrationStatus}
          setError={setError}
          setIrohFetchInput={setIrohFetchInput}
          setRoutePolicy={setRoutePolicy}
          loadIrohInfo={loadIrohInfo}
          irohAddFile={irohAddFile}
          irohShare={irohShare}
          irohFetch={irohFetch}
          irohKeep={irohKeep}
          irohShutdown={irohShutdown}
          irohUnkeep={irohUnkeep}
          irohRegisterTicket={irohRegisterTicket}
        />
      )}
      {activeTab === "advanced" && <AdvancedTools isRunning={isRunning} setError={setError} config={config} onConfigSaved={setConfig} />}

      {/* ── 全局错误 ── */}
        </main>
        {error && <div className="toast-error"><strong>{t("somethingWentWrong")}</strong><span>{error}</span><button onClick={() => setError("")}>×</button></div>}
      </section>
    </div>
  );
}

export default App;
