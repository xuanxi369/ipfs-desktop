import { useTranslation } from "react-i18next";
import { formatError } from "./types";
import { invoke } from "@tauri-apps/api/core";
import { Icon } from "./Icons";

interface IrohNativeProps {
  irohInfo: { peer_id: string; agent_version: string } | null;
  irohCid: string;
  irohTicket: string;
  irohFetchInput: string;
  irohFetchResult: string;
  irohBusy: boolean;
  routePolicy: string;
  migrationStatus: Record<string, number | boolean | string> | null;
  setError: (err: string) => void;
  setIrohFetchInput: (v: string) => void;
  setRoutePolicy: (v: string) => void;
  loadIrohInfo: () => Promise<void>;
  irohAddFile: () => Promise<void>;
  irohShare: () => Promise<void>;
  irohFetch: () => Promise<void>;
  irohKeep: () => Promise<void>;
  irohShutdown: () => Promise<void>;
  irohUnkeep: () => Promise<void>;
  irohRegisterTicket: () => Promise<void>;
}

export default function IrohNative({
  irohInfo, irohCid, irohTicket, irohFetchInput, irohFetchResult, irohBusy, routePolicy, migrationStatus,
  setError, setIrohFetchInput, setRoutePolicy,
  loadIrohInfo, irohAddFile, irohShare, irohFetch, irohKeep, irohShutdown, irohUnkeep, irohRegisterTicket,
}: IrohNativeProps) {
  const { t } = useTranslation();
  const policyDetails: Record<string, string> = {
    LocalFirst: "New local content uses iroh. Kubo stays stopped until an IPFS compatibility operation needs it.",
    Compatible: "Default Auto mode: new content uses iroh; Kubo starts only as an IPFS/IPNS/Gateway compatibility bridge.",
    Mirrored: "Writes to iroh and Kubo, then verifies both copies byte-for-byte before succeeding.",
  };

  async function applyRoutePolicy(policy: string) {
    try {
      const p = await invoke<string>("set_usage_mode", { mode: policy });
      setRoutePolicy(p);
      setError("");
    } catch (e) {
      setError(`Unable to switch route policy to ${policy}: ${formatError(e)}`);
    }
  }

  return (
    <div className="ipns-section">
      <div className="section-header"><div><span className="section-kicker">NATIVE NETWORK</span><h2>Direct Transfer</h2><p className="section-description">Fast content transfer between trusted peers, powered by iroh.</p></div><span className="lab-badge">IROH</span></div>
      <div className="ipns-card">
        <h3><Icon name="iroh"/> {t("irohNative")}</h3>
        <p className="supporting-copy">{t("irohHint")}</p>
        {irohInfo ? (
          <div className="ipns-result">
            {t("peerId")}: <span className="hash-cell">{irohInfo.peer_id}</span>
            <br />
            {t("version")}: {irohInfo.agent_version}
          </div>
        ) : (
          <button onClick={loadIrohInfo} className="btn-small btn-download">
            {t("loadIrohInfo")}
          </button>
        )}
      </div>

      {/* ── 原生添加 + 分享 ── */}
      <div className="ipns-card">
        <h3><Icon name="upload"/> {t("irohAdd")}</h3>
        <div className="input-row">
          <button onClick={irohAddFile} disabled={irohBusy} className="btn-small btn-pin">
            <Icon name="file"/> {t("irohAddFile")}
          </button>
        </div>
        {irohCid && (
          <>
            <div className="ipns-result success">
              CID: <span className="hash-cell">{irohCid}</span>
            </div>
            <div className="input-row">
              <button onClick={irohShare} disabled={irohBusy} className="btn-small btn-download">
                <Icon name="ticket"/> {t("shareTicket")}
              </button>
              <button onClick={irohKeep} disabled={irohBusy} className="btn-small btn-pin">Keep</button>
              <button onClick={irohUnkeep} disabled={irohBusy} className="btn-small">Unkeep</button>
            </div>
          </>
        )}
        {irohTicket && (
          <div className="preview-box">
            <h3>{t("ticket")} ({t("copyTicket")})</h3>
            <pre
              className="ticket-output"
              title={t("copyTicket")}
              onClick={() => { navigator.clipboard?.writeText(irohTicket); }}
            >{irohTicket}</pre>
          </div>
        )}
      </div>

      {/* ── 凭 ticket 收取 ── */}
      <div className="ipns-card">
        <h3><Icon name="download"/> {t("irohReceive")}</h3>
        <div className="input-row">
          <input
            type="text"
            className="cid-input"
            placeholder={t("pasteTicket")}
            value={irohFetchInput}
            onChange={(e) => setIrohFetchInput(e.target.value)}
          />
          <button onClick={irohFetch} disabled={irohBusy || !irohFetchInput.trim()} className="btn-small btn-download">
            <Icon name="download"/> {t("fetchTicket")}
          </button>
          <button onClick={irohRegisterTicket} disabled={irohBusy || !irohFetchInput.trim()} className="btn-small">Register only</button>
        </div>
        {irohFetchResult && <div className="ipns-result success">{irohFetchResult}</div>}
      </div>

      <div className="ipns-card">
        <h3>Lifecycle</h3>
        <button onClick={irohShutdown} disabled={irohBusy} className="btn-small btn-download">Shutdown iroh</button>
      </div>

      {/* ── 双栈路由策略 ── */}
      <div className="ipns-card">
        <h3><Icon name="shuffle"/> Usage mode</h3>
        <div className="input-row">
          <select className="select-input" value={routePolicy} onChange={(e) => applyRoutePolicy(e.target.value)}>
            <option value="LocalFirst">Local first</option>
            <option value="Compatible">IPFS compatible</option>
            <option value="Mirrored">Verified mirror</option>
          </select>
        </div>
        <p className="route-explanation"><strong>{routePolicy}:</strong> {policyDetails[routePolicy]}</p>
        <p className="supporting-copy">{t("routePolicyHint")}</p>
        {migrationStatus && (
          <div className="ipns-result">
            <strong>Migration: {Number(migrationStatus.progress_percent).toFixed(1)}%</strong><br/>
            iroh: {String(migrationStatus.iroh_native)} · mirrored: {String(migrationStatus.mirrored)} · Kubo-only: {String(migrationStatus.kubo_only)}<br/>
            Compatibility: {migrationStatus.ipfs_compatible ? "IPFS ready" : "local only"} · Kubo {migrationStatus.kubo_running ? "running" : "on demand"} · IPNS {migrationStatus.ipns_available ? "available" : "requires Kubo"}
          </div>
        )}
      </div>
    </div>
  );
}
