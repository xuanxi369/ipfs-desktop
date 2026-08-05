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
  irohInfo, irohCid, irohTicket, irohFetchInput, irohFetchResult, irohBusy, routePolicy,
  setError, setIrohFetchInput, setRoutePolicy,
  loadIrohInfo, irohAddFile, irohShare, irohFetch, irohKeep, irohShutdown, irohUnkeep, irohRegisterTicket,
}: IrohNativeProps) {
  const { t } = useTranslation();
  const policyDetails: Record<string, string> = {
    KuboOnly: "All operations use Kubo. Best compatibility with the public IPFS network.",
    Auto: "Uses recorded content origin, local iroh discovery, then CID heuristics. Reads may fall back across backends and registered iroh providers.",
    IrohOnly: "Forces iroh. Kubo CID, Pin and IPNS operations may be unavailable.",
  };

  async function applyRoutePolicy(policy: string) {
    try {
      const p = await invoke<string>("set_route_policy", { policy });
      setRoutePolicy(p);
      setError("");
    } catch (e) {
      setError(`Unable to switch route policy to ${policy}: ${formatError(e)}`);
    }
  }

  return (
    <div className="ipns-section">
      <div className="section-header"><div><span className="section-kicker">EXPERIMENTAL</span><h2>Direct Transfer</h2><p className="section-description">Fast native transfers for trusted peers, powered by iroh.</p></div><span className="lab-badge">IROH LAB</span></div>
      <div className="ipns-card">
        <h3><Icon name="iroh"/> {t("irohNative")}</h3>
        <p style={{ fontSize: "0.85em", opacity: 0.7 }}>{t("irohHint")}</p>
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
              style={{ cursor: "pointer", whiteSpace: "pre-wrap", wordBreak: "break-all" }}
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
        <h3><Icon name="shuffle"/> {t("routePolicy")}</h3>
        <div className="input-row">
          <select className="select-input" value={routePolicy} onChange={(e) => applyRoutePolicy(e.target.value)}>
            <option value="KuboOnly">KuboOnly</option>
            <option value="Auto">Auto</option>
            <option value="IrohOnly">IrohOnly</option>
          </select>
        </div>
        <p className="route-explanation"><strong>{routePolicy}:</strong> {policyDetails[routePolicy]}</p>
        <p style={{ fontSize: "0.85em", opacity: 0.7 }}>{t("routePolicyHint")}</p>
      </div>
    </div>
  );
}
