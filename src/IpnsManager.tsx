import { useTranslation } from "react-i18next";

interface IpnsManagerProps {
  isRunning: boolean;
  ipnsCid: string;
  ipnsKeyName: string;
  ipnsLifetime: string;
  ipnsResolveName: string;
  ipnsResolveResult: string;
  ipnsPublishResult: string;
  keyList: { public_key: string; ipns_name: string; label: string }[];
  newKeyLabel: string;
  setIpnsCid: (v: string) => void;
  setIpnsKeyName: (v: string) => void;
  setIpnsLifetime: (v: string) => void;
  setIpnsResolveName: (v: string) => void;
  setNewKeyLabel: (v: string) => void;
  publishIpns: () => Promise<void>;
  resolveIpns: () => Promise<void>;
  generateNewKey: () => Promise<void>;
  loadKeyList: () => Promise<void>;
  deleteKeyByLabel: (label: string) => Promise<void>;
}

export default function IpnsManager({
  isRunning, ipnsCid, ipnsKeyName, ipnsLifetime,
  ipnsResolveName, ipnsResolveResult, ipnsPublishResult,
  keyList, newKeyLabel,
  setIpnsCid, setIpnsKeyName, setIpnsLifetime,
  setIpnsResolveName, setNewKeyLabel,
  publishIpns, resolveIpns, generateNewKey, loadKeyList, deleteKeyByLabel,
}: IpnsManagerProps) {
  const { t } = useTranslation();

  return (
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
  );
}
