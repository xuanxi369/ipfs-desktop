import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { TFunction } from "i18next";
import { formatBytes, formatError } from "../types";

export function useIroh(setError: (message: string) => void, t: TFunction) {
  const [irohInfo, setIrohInfo] = useState<{ peer_id: string; agent_version: string } | null>(null);
  const [irohCid, setIrohCid] = useState("");
  const [irohTicket, setIrohTicket] = useState("");
  const [irohFetchInput, setIrohFetchInput] = useState("");
  const [irohFetchResult, setIrohFetchResult] = useState("");
  const [irohBusy, setIrohBusy] = useState(false);
  const [routePolicy, setRoutePolicy] = useState("Compatible");
  const [migrationStatus, setMigrationStatus] = useState<Record<string, number | boolean | string> | null>(null);

  async function loadIrohInfo() {
    try { setIrohInfo(await invoke("iroh_node_info")); setError(""); }
    catch (e) { setIrohInfo(null); setError(`iroh: ${formatError(e)}`); }
  }
  async function loadRoutePolicy() {
    try {
      setRoutePolicy(await invoke<string>("get_usage_mode"));
      setMigrationStatus(await invoke("get_migration_status"));
    } catch { /* optional */ }
  }
  async function irohAddFile() {
    try {
      const selected = await open({ multiple: false, title: t("irohAddFile") });
      if (!selected || typeof selected !== "string") return;
      setIrohBusy(true);
      const out = await invoke<{ cid: string }>("iroh_add_file", { filePath: selected });
      setIrohCid(out.cid); setIrohTicket(""); setError("");
    } catch (e) { setError(`iroh add: ${formatError(e)}`); } finally { setIrohBusy(false); }
  }
  async function irohShare() {
    if (!irohCid.trim()) return;
    try { setIrohBusy(true); setIrohTicket(await invoke("iroh_share", { cid: irohCid.trim() })); setError(""); }
    catch (e) { setError(`iroh share: ${formatError(e)}`); } finally { setIrohBusy(false); }
  }
  async function irohFetch() {
    if (!irohFetchInput.trim()) return;
    try {
      setIrohBusy(true);
      const savePath = await save({ title: t("saveDownloadAs") });
      const res = await invoke<{ size: number; saved: string | null }>("iroh_fetch_ticket", { ticket: irohFetchInput.trim(), savePath: savePath || null });
      setIrohFetchResult(`${formatBytes(res.size)} — ${res.saved ? `${t("saved")}: ${res.saved}` : t("notSaved")}`); setError("");
    } catch (e) { setError(`iroh fetch: ${formatError(e)}`); } finally { setIrohBusy(false); }
  }
  async function simple(command: string, cid?: string) {
    try { await invoke(command, cid ? { cid } : undefined); setError(""); }
    catch (e) { setError(`iroh: ${formatError(e)}`); }
  }
  async function irohRegisterTicket() {
    if (!irohFetchInput.trim()) return;
    try { const cid = await invoke<string>("iroh_register_ticket", { ticket: irohFetchInput.trim() }); setIrohFetchResult(`Registered provider for ${cid}`); setError(""); }
    catch (e) { setError(`iroh register: ${formatError(e)}`); }
  }

  return { irohInfo, irohCid, irohTicket, irohFetchInput, irohFetchResult, irohBusy, routePolicy, migrationStatus,
    setIrohFetchInput, setRoutePolicy, loadIrohInfo, loadRoutePolicy, irohAddFile, irohShare, irohFetch,
    irohKeep: () => irohCid.trim() ? simple("iroh_keep", irohCid.trim()) : Promise.resolve(),
    irohUnkeep: () => irohCid.trim() ? simple("iroh_unkeep", irohCid.trim()) : Promise.resolve(),
    irohShutdown: () => simple("iroh_shutdown"), irohRegisterTicket };
}
