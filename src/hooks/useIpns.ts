import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatError } from "../types";

export type IpnsKey = { public_key: string; ipns_name: string; label: string };

export function useIpns(setError: (message: string) => void) {
  const [ipnsCid, setIpnsCid] = useState("");
  const [ipnsKeyName, setIpnsKeyName] = useState("self");
  const [ipnsLifetime, setIpnsLifetime] = useState("24h");
  const [ipnsResolveName, setIpnsResolveName] = useState("");
  const [ipnsResolveResult, setIpnsResolveResult] = useState("");
  const [ipnsPublishResult, setIpnsPublishResult] = useState("");
  const [keyList, setKeyList] = useState<IpnsKey[]>([]);
  const [newKeyLabel, setNewKeyLabel] = useState("");

  async function loadKeyList() {
    try { setKeyList((await invoke<IpnsKey[]>("list_keys")) || []); }
    catch (e) { setError(`Key list failed: ${formatError(e)}`); }
  }
  async function publishIpns() {
    if (!ipnsCid.trim()) return;
    try {
      const result = await invoke<{ Name: string; Value: string }>("ipns_publish", {
        cid: ipnsCid.trim(), keyName: ipnsKeyName.trim() || "self", lifetime: ipnsLifetime,
      });
      setIpnsPublishResult(`${result.Name} → ${result.Value}`); setError("");
    } catch (e) { setError(`IPNS publish failed: ${formatError(e)}`); }
  }
  async function resolveIpns() {
    if (!ipnsResolveName.trim()) return;
    try { setIpnsResolveResult((await invoke<{ Path: string }>("ipns_resolve", { name: ipnsResolveName.trim() })).Path); setError(""); }
    catch (e) { setError(`IPNS resolve failed: ${formatError(e)}`); }
  }
  async function generateNewKey() {
    if (!newKeyLabel.trim()) return;
    try { await invoke("generate_key", { label: newKeyLabel.trim() }); setNewKeyLabel(""); await loadKeyList(); setError(""); }
    catch (e) { setError(`Key generation failed: ${formatError(e)}`); }
  }
  async function deleteKeyByLabel(label: string) {
    try { await invoke("delete_key", { label }); await loadKeyList(); }
    catch (e) { setError(`Key delete failed: ${formatError(e)}`); }
  }

  return { ipnsCid, ipnsKeyName, ipnsLifetime, ipnsResolveName, ipnsResolveResult, ipnsPublishResult,
    keyList, newKeyLabel, setIpnsCid, setIpnsKeyName, setIpnsLifetime, setIpnsResolveName,
    setNewKeyLabel, publishIpns, resolveIpns, generateNewKey, loadKeyList, deleteKeyByLabel };
}
