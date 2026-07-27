import { useTranslation } from "react-i18next";
import { AddResult, DownloadProgress, UploadProgress, formatBytes } from "./types";

interface FilesProps {
  isRunning: boolean;
  uploading: boolean;
  downloadCid: string;
  downloadProgress: DownloadProgress | null;
  downloading: boolean;
  catResult: string;
  uploadProgress: UploadProgress | null;
  uploads: AddResult[];
  routeHint: string;
  setDownloadCid: (v: string) => void;
  selectAndUpload: () => Promise<void>;
  catByCid: () => Promise<void>;
  downloadByCid: () => Promise<void>;
}

export default function Files({
  isRunning, uploading, downloadCid, downloadProgress, downloading,
  catResult, uploadProgress, uploads, routeHint,
  setDownloadCid, selectAndUpload, catByCid, downloadByCid,
}: FilesProps) {
  const { t } = useTranslation();

  return (
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
        {routeHint && (
          <div style={{ fontSize: "0.82em", opacity: 0.75, marginTop: "4px" }}>
            🔀 {t("routeTo")}: <strong>{routeHint}</strong>
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
                  <td className="hash-cell">{f.Hash}</td>
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
