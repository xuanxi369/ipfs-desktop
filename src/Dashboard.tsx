import { useTranslation } from "react-i18next";
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { loadBenchmarkHistory, recordBenchmark } from "./benchmarkHistory";
import { Icon } from "./Icons";
import PeerMap from "./PeerMap";
import {
  DaemonStatus, AppConfig, DashboardStats, NodeIdentityInfo, NodeHealth,
  formatError, formatBytes, formatRate, formatUptime,
} from "./types";

interface DashboardProps {
  status: DaemonStatus;
  config: AppConfig | null;
  isRunning: boolean;
  dashboard: DashboardStats | null;
  dashLoading: boolean;
  cacheHit: boolean;
  identity: NodeIdentityInfo | null;
  editingLabel: boolean;
  labelDraft: string;
  health: NodeHealth | null;
  backendCaps: Record<string, unknown> | null;
  benchResult: Record<string, unknown> | null;
  benchRunning: boolean;
  compatResult: Record<string, unknown> | null;
  compatRunning: boolean;
  proxyStats: { total_requests: number; cache_hits: number; api_calls: number; circuit_open_count: number; avg_latency_ms: number } | null;
  offlineCount: number;
  bwConfig: { max_connections: number; max_streams: number; upload_limit: number; download_limit: number; enabled: boolean };
  bwStatus: { rate_in: number; rate_out: number; total_in: number; total_out: number } | null;
  setError: (err: string) => void;
  setBackendCaps: (c: Record<string, unknown> | null) => void;
  setBenchResult: (r: Record<string, unknown> | null) => void;
  setBenchRunning: (r: boolean) => void;
  setCompatResult: (r: Record<string, unknown> | null) => void;
  setCompatRunning: (r: boolean) => void;
  setOfflineCount: (n: number) => void;
  setBwConfig: (c: { max_connections: number; max_streams: number; upload_limit: number; download_limit: number; enabled: boolean }) => void;
  setEditingLabel: (v: boolean) => void;
  setLabelDraft: (v: string) => void;
  loadDashboard: () => Promise<void>;
  loadIdentity: () => Promise<void>;
  loadHealth: () => Promise<void>;
  saveLabel: () => Promise<void>;
  exportIdentity: () => Promise<void>;
  startDaemon: () => Promise<void>;
  stopDaemon: () => Promise<void>;
  restartDaemon: () => Promise<void>;
  loadStatus: () => Promise<void>;
  openWebui: () => Promise<void>;
  toggleAutoLaunch: () => Promise<void>;
}

export default function Dashboard({
  status, config, isRunning, dashboard, dashLoading, cacheHit,
  identity, editingLabel, labelDraft, health,
  backendCaps, benchResult, benchRunning, compatResult, compatRunning,
  proxyStats, offlineCount, bwConfig, bwStatus,
  setError, setBackendCaps, setBenchResult, setBenchRunning,
  setCompatResult, setCompatRunning, setOfflineCount, setBwConfig,
  setEditingLabel, setLabelDraft,
  loadDashboard, loadIdentity, loadHealth, saveLabel, exportIdentity,
  startDaemon, stopDaemon, restartDaemon, loadStatus, openWebui,
  toggleAutoLaunch,
}: DashboardProps) {
  const [benchmarkHistory, setBenchmarkHistory] = useState(loadBenchmarkHistory);
  const { t } = useTranslation();

  const getStatusColor = () => {
    switch (status.type) {
      case "Running": return "#4caf50";
      case "Starting": case "Stopping": return "#ff9800";
      case "Failed": return "#f44336";
      default: return "#9e9e9e";
    }
  };

  return (
    <>
      <div className="page-intro dashboard-intro">
        <div><span className="section-kicker">NODE</span><h2>{t("nodeDashboard")}</h2><p>{t("daemonStatus")} · {t("network")} · {t("contentItems")}</p></div>
        <span className={`availability-badge ${isRunning ? "ready" : ""}`}>{isRunning ? t("nodeOnline") : t("nodeOffline")}</span>
      </div>
      {/* ── Phase D1: 节点身份卡 ── */}
      <div className="status-card">
        <h2><Icon name="identity"/> {t("nodeIdentity")}</h2>
        {identity ? (
          <div className="status-details">
            <div style={{ display: "flex", alignItems: "center", gap: "8px", marginBottom: "6px", flexWrap: "wrap" }}>
              {editingLabel ? (
                <>
                  <input className="cid-input small-input" value={labelDraft}
                    placeholder={t("nodeLabel")}
                    onChange={(e) => setLabelDraft(e.target.value)} />
                  <button className="btn-small btn-download" onClick={saveLabel} disabled={!labelDraft.trim()}><Icon name="save"/> {t("save")}</button>
                  <button className="btn-small" onClick={() => setEditingLabel(false)}><Icon name="xmark"/></button>
                </>
              ) : (
                <>
                  <strong style={{ fontSize: "1.1em" }}>{identity.label}</strong>
                  <button className="btn-small" onClick={() => { setLabelDraft(identity.label); setEditingLabel(true); }}><Icon name="edit"/> {t("edit")}</button>
                </>
              )}
            </div>
            <p style={{ fontSize: "0.8em", opacity: 0.65 }}>{t("since")}: {new Date(identity.created_at * 1000).toLocaleDateString()}</p>
            {identity.kubo_peer_id && <p className="hash-cell" title={identity.kubo_peer_id}>Kubo: {identity.kubo_peer_id}</p>}
            {identity.iroh_node_id && <p className="hash-cell" title={identity.iroh_node_id}>iroh: {identity.iroh_node_id}</p>}
            <button className="btn-small btn-pin" style={{ marginTop: "6px" }} onClick={exportIdentity}><Icon name="clipboard"/> {t("exportIdentity")}</button>
          </div>
        ) : (
          <button className="btn-small btn-download" onClick={loadIdentity}>{t("loadIdentity")}</button>
        )}
      </div>

      {/* ── Phase D3: 节点健康度 ── */}
      {health && (
        <div className="status-card">
          <h2><Icon name="heart"/> {t("nodeHealth")}</h2>
          <div className="status-details">
            <p><Icon name="clock"/> {t("appUptime")}: <strong>{formatUptime(health.app_uptime_secs)}</strong>
              {health.daemon_uptime_secs != null && <> · {t("nodeUptime")}: <strong>{formatUptime(health.daemon_uptime_secs)}</strong></>}
            </p>
            {health.num_objects != null && (
              <p><Icon name="box"/> {t("numObjects")}: {health.num_objects} · {t("repoSize")}: {formatBytes(health.repo_size ?? 0)}</p>
            )}
            {health.peers != null && <p><Icon name="link"/> {t("connectedPeers")}: {health.peers}</p>}
            {(health.bytes_in != null || health.bytes_out != null) && (
              <p><Icon name="download"/> {formatBytes(health.bytes_in ?? 0)} · <Icon name="upload"/> {formatBytes(health.bytes_out ?? 0)} <span style={{ opacity: 0.6 }}>({t("contribution")})</span></p>
            )}
            {health.iroh_content_count != null && <p><Icon name="iroh"/> iroh {t("contentItems")}: {health.iroh_content_count}</p>}
            <button className="btn-small btn-download" style={{ marginTop: "4px" }} onClick={loadHealth}><Icon name="refresh"/> {t("refresh")}</button>
          </div>
        </div>
      )}

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
            {dashLoading ? <span className="spinner"/> : <Icon name="refresh"/>} {t("refresh")}
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
                    <span className="dash-value">{dashboard.repo.repo_size != null ? formatBytes(dashboard.repo.repo_size) : "N/A"}</span>
                  </div>
                  <div className="dash-stat">
                    <span className="dash-label">{t("numObjects")}</span>
                    <span className="dash-value">{dashboard.repo.num_objects != null ? dashboard.repo.num_objects.toLocaleString() : "N/A"}</span>
                  </div>
                </>
              ) : <div className="dash-na">N/A</div>}
            </div>

            {/* 网络连接 */}
            <div className="dash-card">
              <div className="dash-card-title">{t("network")}</div>
              {Array.isArray(dashboard.peers?.peers) ? (
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

      <PeerMap isRunning={isRunning} setError={setError} />

      {/* Runtime capabilities; product-level selection lives in Usage Mode. */}
      <div className="backend-bar">
        <span className="backend-label"><Icon name="link"/> Runtime compatibility:</span>
        <span className="backend-caps-badge">iroh primary · Kubo on demand</span>
        <button onClick={async () => {
          try {
            const caps = await invoke<Record<string, unknown>>("get_backend_capabilities");
            setBackendCaps(caps);
          } catch (e) { setError(formatError(e)); }
        }} className="btn-small">{t("viewCapabilities")}</button>
        {backendCaps && (
          <span className="backend-caps-badge" title={JSON.stringify(backendCaps, null, 2)}>
            IPNS:{(backendCaps as any).ipns ? <Icon name="check"/> : <Icon name="xmark"/>} Pin:{(backendCaps as any).pinning ? <Icon name="check"/> : <Icon name="xmark"/>}
          </span>
        )}
        <button onClick={async () => {
          try {
            setBenchRunning(true);
            const result = await invoke<Record<string, unknown>>("run_benchmark");
            setBenchResult(result);
            setBenchmarkHistory(recordBenchmark(result));
          } catch (e) { setError(formatError(e)); }
          finally { setBenchRunning(false); }
        }} className="btn-small btn-download" disabled={benchRunning}>
          {benchRunning ? <span className="spinner"/> : <Icon name="zap"/>} {t("runBenchmark")}
        </button>
        <button onClick={async () => {
          try {
            setCompatRunning(true);
            const result = await invoke<Record<string, unknown>>("run_compat_test");
            setCompatResult(result);
          } catch (e) { setError(formatError(e)); }
          finally { setCompatRunning(false); }
        }} className="btn-small btn-pin" disabled={compatRunning}>
          {compatRunning ? <span className="spinner"/> : <Icon name="flask"/>} {t("runCompatTest")}
        </button>
      </div>

      {/* ── Phase 3: 代理统计 + 离线队列 + 带宽控制 ── */}
      {isRunning && (
        <>
        <div className="phase3-grid">

          {/* 代理统计 */}
          <div className="phase3-card">
            <div className="dash-card-title"><Icon name="zap"/> {t("proxyStats")}</div>
            <div className="phase3-row">
              <span>{t("cacheHitRate")}</span>
              <span className="dash-value">
                {proxyStats && proxyStats.total_requests > 0
                  ? `${Math.round(proxyStats.cache_hits / proxyStats.total_requests * 100)}%`
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
                {proxyStats && proxyStats.circuit_open_count > 0 ? <><Icon name="alert"/> {proxyStats.circuit_open_count}x</> : <><Icon name="check"/> OK</>}
              </span>
            </div>
          </div>

          {/* 离线队列 */}
          <div className="phase3-card">
            <div className="dash-card-title"><Icon name="clipboard"/> {t("offlineQueue")}</div>
            <div className="phase3-row">
              <span>{t("pendingOps")}</span>
              <span className={`dash-value ${offlineCount > 0 ? "orange" : "green"}`}>
                {offlineCount}
              </span>
            </div>
            <button onClick={async () => {
              try {
                const r = await invoke<{ count: number }>("get_offline_queue");
                setOfflineCount(r.count);
                if (r.count > 0) await invoke("flush_offline_queue");
              } catch (e) { setError(formatError(e)); }
            }} disabled={offlineCount === 0} className="btn-small btn-pin"
              style={{ marginTop: 8, width: "100%" }}>
              {offlineCount > 0 ? <><Icon name="play"/> {t("flushQueue")} ({offlineCount})</> : t("queueEmpty")}
            </button>
          </div>

          {/* 带宽控制 */}
          <div className="phase3-card">
            <div className="dash-card-title"><Icon name="signal"/> {t("bandwidthControl")}</div>
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
                  const newCfg = { ...bwConfig, max_connections: v };
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
            <h3><Icon name="zap"/> {t("benchmarkResults")}</h3>
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
        {benchmarkHistory.length > 0 && <div className="bench-result benchmark-history"><h3>Benchmark history</h3>{benchmarkHistory.map((r) => <div className="bench-row" key={r.timestamp}><span>{new Date(r.timestamp).toLocaleString()}</span><span>{r.winner || "—"}</span><span>{r.speedup_ratio ? `${r.speedup_ratio.toFixed(2)}x` : "—"}</span><span>{r.total_duration_ms ?? "—"}ms</span></div>)}</div>}

        {/* 兼容性测试结果 */}
        {compatResult && (
          <div className="compat-result">
            <h3><Icon name="flask"/> {t("compatResults")}</h3>
            <div className="compat-score">
              {(compatResult as any).compatibility_score?.toFixed(0)}% {t("compatible")}
            </div>
            <div className="compat-summary">
              <span><Icon name="check"/> {(compatResult as any).passed}</span>
              <span><Icon name="xmark"/> {(compatResult as any).failed}</span>
              <span>— {(compatResult as any).skipped}</span>
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
  );
}
