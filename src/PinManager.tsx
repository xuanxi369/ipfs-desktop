import { useTranslation } from "react-i18next";
import { PinEntry } from "./types";

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

  return (
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
  );
}
