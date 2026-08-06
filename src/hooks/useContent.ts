import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { TFunction } from "i18next";
import type { AddResult, ContentRecord, DownloadProgress, PinEntry, PinList, UploadProgress } from "../types";
import { formatError } from "../types";

export function useContent(setError: (message: string) => void, t: TFunction) {
  const [uploads, setUploads] = useState<AddResult[]>([]);
  const [contentRecords, setContentRecords] = useState<ContentRecord[]>([]);
  const [uploading, setUploading] = useState(false);
  const [uploadProgress, setUploadProgress] = useState<UploadProgress | null>(null);
  const [downloadCid, setDownloadCid] = useState("");
  const [downloadProgress, setDownloadProgress] = useState<DownloadProgress | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [catResult, setCatResult] = useState("");
  const [routeHint, setRouteHint] = useState("");
  const [pinList, setPinList] = useState<PinEntry[]>([]);
  const [pinLoading, setPinLoading] = useState(false);
  const [pinCid, setPinCid] = useState("");

  async function loadContentRecords() {
    try { setContentRecords(await invoke<ContentRecord[]>("list_content")); }
    catch (e) { setError(`Content index failed: ${formatError(e)}`); }
  }
  async function removeContentRecord(cid: string) {
    try { await invoke("remove_content_record", { cid }); await loadContentRecords(); }
    catch (e) { setError(`Content record: ${formatError(e)}`); }
  }
  const selectAndUpload = useCallback(async () => {
    try {
      const selected = await open({ multiple: true, title: t("selectFiles") });
      if (!selected) return;
      setUploading(true); setUploadProgress(null);
      const results: AddResult[] = [];
      for (const filePath of Array.isArray(selected) ? selected : [selected]) {
        results.push(await invoke<AddResult>("add_file_with_progress", { filePath }));
      }
      setUploads((previous) => [...previous, ...results]);
      await loadContentRecords(); setError("");
    } catch (e) { setError(`Upload failed: ${formatError(e)}`); }
    finally { setUploading(false); setUploadProgress(null); }
  }, [t]);
  async function downloadByCid() {
    if (!downloadCid.trim()) return;
    try {
      setDownloading(true); setDownloadProgress({ cid: downloadCid, loaded: 0, total: null });
      const savePath = await save({ defaultPath: downloadCid, title: t("saveDownloadAs") });
      if (!savePath) return;
      await invoke("download_file", { cid: downloadCid.trim(), savePath }); setError("");
    } catch (e) { setError(`Download failed: ${formatError(e)}`); }
    finally { setDownloading(false); }
  }
  async function catByCid() {
    if (!downloadCid.trim()) return;
    try {
      const bytes = new Uint8Array(await invoke<number[]>("cat_file", { cid: downloadCid.trim() }));
      const text = new TextDecoder("utf-8", { fatal: false }).decode(bytes);
      const binary = bytes.includes(0) || text.includes("\uFFFD");
      if (binary) {
        const hex = Array.from(bytes.slice(0, 256), (byte) => byte.toString(16).padStart(2, "0")).join(" ");
        setCatResult(`[${t("binaryContent")} - ${bytes.length} ${t("bytes")}]\n${hex}${bytes.length > 256 ? " …" : ""}`);
      } else { setCatResult(text.slice(0, 5000)); }
      setError("");
    } catch (e) { setError(`Cat failed: ${formatError(e)}`); }
  }
  async function loadPins() {
    try { setPinLoading(true); setPinList((await invoke<PinList>("get_pin_list")).pins || []); setError(""); }
    catch (e) { setError(`Pin list failed: ${formatError(e)}`); }
    finally { setPinLoading(false); }
  }
  async function addPinByCid() {
    if (!pinCid.trim()) return;
    try { await invoke("add_pin", { cid: pinCid.trim() }); setPinCid(""); await loadPins(); }
    catch (e) { setError(`Pin add failed: ${formatError(e)}`); }
  }
  async function removePinByCid(cid: string) {
    try { await invoke("remove_pin", { cid }); await loadPins(); }
    catch (e) { setError(`Pin remove failed: ${formatError(e)}`); }
  }

  useEffect(() => {
    const cid = downloadCid.trim();
    if (!cid) { setRouteHint(""); return; }
    const timer = setTimeout(async () => {
      try { setRouteHint(await invoke<string>("get_backend_route", { cid })); } catch { setRouteHint(""); }
    }, 300);
    return () => clearTimeout(timer);
  }, [downloadCid]);

  return { uploads, contentRecords, uploading, uploadProgress, setUploadProgress, downloadCid, setDownloadCid,
    downloadProgress, setDownloadProgress, downloading, catResult, routeHint, pinList, pinLoading, pinCid,
    setPinCid, loadContentRecords, removeContentRecord, selectAndUpload, downloadByCid, catByCid,
    loadPins, addPinByCid, removePinByCid };
}
