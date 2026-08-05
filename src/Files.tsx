import { useTranslation } from "react-i18next";
import { AddResult, ContentRecord, DownloadProgress, UploadProgress, formatBytes, shortHash } from "./types";
import { useState } from "react";
import { Icon } from "./Icons";

interface FilesProps {
  isRunning: boolean;
  uploading: boolean;
  downloadCid: string;
  downloadProgress: DownloadProgress | null;
  downloading: boolean;
  catResult: string;
  uploadProgress: UploadProgress | null;
  uploads: AddResult[];
  contentRecords: ContentRecord[];
  loadContentRecords: () => Promise<void>;
  routeHint: string;
  setDownloadCid: (v: string) => void;
  selectAndUpload: () => Promise<void>;
  catByCid: () => Promise<void>;
  downloadByCid: () => Promise<void>;
  removeContentRecord: (cid: string) => Promise<void>;
}

export default function Files({
  isRunning, uploading, downloadCid, downloadProgress, downloading,
  catResult, uploadProgress, uploads, routeHint, contentRecords, loadContentRecords,
  setDownloadCid, selectAndUpload, catByCid, downloadByCid, removeContentRecord,
}: FilesProps) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState<string | null>(null);
  const copy = async (hash: string) => { await navigator.clipboard?.writeText(hash); setCopied(hash); window.setTimeout(() => setCopied(null), 1400); };

  return (
    <div className="files-section">
      <div className="page-intro"><div><span className="section-kicker">CONTENT</span><h2>{t("files")}</h2><p>{t("filesDescription")}</p></div><span className={`availability-badge ${isRunning ? "ready" : ""}`}>{isRunning ? t("ready") : t("nodeOffline")}</span></div>
      <div className="section-header content-toolbar"><div><h3>{t("contentLibrary")}</h3><p className="section-description">{contentRecords.length} {t("indexedItems")}</p></div><button className="btn-small" onClick={loadContentRecords}><Icon name="activity"/> {t("refresh")}</button></div>
      {contentRecords.length > 0 && <div className="content-table"><table><thead><tr><th>{t("name")}</th><th>CID</th><th>{t("size")}</th><th>{t("backend")}</th><th>{t("added")}</th><th> </th></tr></thead><tbody>{contentRecords.map((f)=><tr key={f.cid}><td>{f.name}</td><td><button className="hash-button" title={f.cid} onClick={()=>copy(f.cid)}>{shortHash(f.cid)} <Icon name="copy"/></button></td><td>{formatBytes(f.size)}</td><td><span className="pin-type-badge">{f.backend}</span></td><td>{new Date(f.added_at*1000).toLocaleDateString()}</td><td><button className="btn-small btn-danger" onClick={() => removeContentRecord(f.cid)}>Remove</button></td></tr>)}</tbody></table></div>}
      {/* ── 上传 ── */}
      <h2>{t("uploadFiles")}</h2>
      <div className="drop-zone" onClick={selectAndUpload}>
        <p>{t("dropHere")}</p>
        <button className="btn-secondary" disabled={uploading}>
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
          />
          <button onClick={catByCid} disabled={!downloadCid.trim()} className="btn-small">
            <Icon name="search"/> {t("preview")}
          </button>
          <button onClick={downloadByCid} disabled={!downloadCid.trim() || downloading} className="btn-small btn-download">
            {downloading ? <span className="spinner"/> : <Icon name="download"/>} {t("download")}
          </button>
        </div>
        {routeHint && (
          <div style={{ fontSize: "0.82em", opacity: 0.75, marginTop: "4px" }}>
            <Icon name="shuffle"/> {t("routeTo")}: <strong>{routeHint}</strong>
          </div>
        )}
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
                  <td><button className="hash-button" title={f.Hash} onClick={() => copy(f.Hash)}>{shortHash(f.Hash)} {copied === f.Hash ? <span className="copied-label">{t("copied")}</span> : <Icon name="copy"/>}</button></td>
                  <td>{f.Size}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
