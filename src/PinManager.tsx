import { useTranslation } from "react-i18next";
import { PinEntry, shortHash } from "./types";
import { useState } from "react";
import { Icon } from "./Icons";

interface PinManagerProps {
  isRunning: boolean;
  pinList: PinEntry[];
  pinLoading: boolean;
  pinCid: string;
  setPinCid: (v: string) => void;
  loadPins: () => Promise<void>;
  addPinByCid: () => Promise<void>;
  removePinByCid: (cid: string) => Promise<void>;
}

export default function PinManager({
  isRunning, pinList, pinLoading, pinCid,
  setPinCid, loadPins, addPinByCid, removePinByCid,
}: PinManagerProps) {
  const { t } = useTranslation();
  const [confirmCid, setConfirmCid] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const copy = async (cid: string) => { await navigator.clipboard?.writeText(cid); setCopied(cid); window.setTimeout(() => setCopied(null), 1400); };

  return (
    <div className="pins-section">
      <div className="section-header">
        <div><span className="section-kicker">RETENTION</span><h2>{t("pinManagement")}</h2><p className="section-description">Keep important content available on this node.</p></div>
        <button onClick={loadPins} disabled={!isRunning || pinLoading} className="btn-small">
          {pinLoading ? <span className="spinner"/> : <Icon name="activity"/>} {t("refresh")}
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
          <Icon name="pins"/> {t("pin")}
        </button>
      </div>

      {/* Pin 列表 */}
      {pinLoading && <div className="table-skeleton" aria-label={t("loading")}><span/><span/><span/></div>}
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
                  <td><button className="hash-button" title={pin.Cid} onClick={() => copy(pin.Cid)}>{shortHash(pin.Cid)} {copied === pin.Cid ? <span className="copied-label">{t("copied")}</span> : <Icon name="copy"/>}</button></td>
                  <td><span className={`pin-type-badge ${pin.Type}`}>{pin.Type}</span></td>
                  <td>
                    <button
                      onClick={() => setConfirmCid(pin.Cid)}
                      className="btn-small btn-danger"
                      disabled={!isRunning}
                    >
                      {t("unpin")}
                    </button>
                    {confirmCid === pin.Cid && <div className="confirm-popover"><span>{t("confirmUnpin")}</span><button onClick={() => { removePinByCid(pin.Cid); setConfirmCid(null); }}>{t("confirm")}</button><button onClick={() => setConfirmCid(null)}>{t("cancel")}</button></div>}
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
  );
}
