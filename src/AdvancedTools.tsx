import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AppConfig, formatError } from "./types";

export default function AdvancedTools({ setError, config, onConfigSaved }: { isRunning: boolean; setError: (e: string) => void; config: AppConfig | null; onConfigSaved: (c: AppConfig) => void }) {
  const [mfsPath, setMfsPath] = useState("/");
  const [mfsResult, setMfsResult] = useState("");
  const [binaryHash, setBinaryHashValue] = useState("");
  const [binaryInfo, setBinaryInfo] = useState("");
  const [mfsContent, setMfsContent] = useState("");
  const [mfsSource, setMfsSource] = useState("");
  const [mfsDest, setMfsDest] = useState("");
  const [apiAddress, setApiAddress] = useState(config?.api_addr ?? "http://127.0.0.1:5001");
  const [gatewayAddress, setGatewayAddress] = useState(config?.gateway_addr ?? "http://127.0.0.1:8080");
  const [allowRemoteApi, setAllowRemoteApi] = useState(config?.allow_remote_api ?? false);

  async function call(name: string, args: Record<string, unknown> = {}) {
    try { const result = await invoke(name, args); setMfsResult(JSON.stringify(result, null, 2)); setError(""); }
    catch (e) { setError(formatError(e)); }
  }

  return <div className="ipns-section">
    <div className="section-header"><div><span className="section-kicker">ADVANCED</span><h2>MFS & Diagnostics</h2></div></div>
    <div className="ipns-card">
      <h3>API and Gateway</h3>
      <div className="input-row"><input aria-label="API address" className="cid-input" value={apiAddress} onChange={e => setApiAddress(e.target.value)} /><input aria-label="Gateway address" className="cid-input" value={gatewayAddress} onChange={e => setGatewayAddress(e.target.value)} />
        <label><input aria-label="Allow remote API" type="checkbox" checked={allowRemoteApi} onChange={e => setAllowRemoteApi(e.target.checked)} /> Allow remote API</label>
        <button className="btn-small btn-pin" disabled={!config} onClick={async () => { if (!config) return; const next = { ...config, api_addr: apiAddress.trim(), gateway_addr: gatewayAddress.trim(), allow_remote_api: allowRemoteApi }; try { await invoke("update_config", { newConfig: next }); onConfigSaved(next); setError(""); } catch (e) { setError(formatError(e)); } }}>Save endpoints</button></div>
      <p className="section-description">Remote access is disabled by default. When enabled, endpoints must use HTTPS and resolve only to public IPs. Set IPFS_API_AUTHORIZATION for reverse-proxy authentication; system HTTP proxies are bypassed.</p>
    </div>
    <div className="ipns-card">
      <h3>Mutable File System</h3>
      <div className="input-row"><input className="cid-input" value={mfsPath} onChange={e => setMfsPath(e.target.value)} placeholder="/path" />
        <button className="btn-small" onClick={() => call("mfs_ls", { path: mfsPath })}>List</button>
        <button className="btn-small" onClick={() => call("mfs_stat", { path: mfsPath })}>Stat</button>
        <button className="btn-small btn-pin" onClick={() => call("mfs_mkdir", { path: mfsPath, parents: true })}>Mkdir</button>
        <button className="btn-small btn-danger" disabled={mfsPath === "/"} onClick={() => call("mfs_rm", { path: mfsPath, recursive: true })}>Remove</button>
      </div>
      <div className="input-row"><textarea className="cid-input" value={mfsContent} onChange={e => setMfsContent(e.target.value)} placeholder="Content for MFS write" />
        <button className="btn-small btn-pin" onClick={() => call("mfs_write", { path: mfsPath, content: Array.from(new TextEncoder().encode(mfsContent)), create: true, truncate: true })}>Write</button>
        <button className="btn-small" onClick={async () => { try { const b = await invoke<number[]>("mfs_read", { path: mfsPath }); setMfsContent(new TextDecoder().decode(new Uint8Array(b))); setMfsResult("Read complete"); } catch (e) { setError(formatError(e)); } }}>Read</button>
      </div>
      <div className="input-row"><input className="cid-input" value={mfsSource} onChange={e => setMfsSource(e.target.value)} placeholder="Source /ipfs/... or MFS path" /><input className="cid-input" value={mfsDest} onChange={e => setMfsDest(e.target.value)} placeholder="Destination /path" />
        <button className="btn-small" onClick={() => call("mfs_cp", { source: mfsSource, dest: mfsDest })}>Copy</button>
        <button className="btn-small" onClick={() => call("mfs_mv", { source: mfsSource, dest: mfsDest })}>Move</button>
      </div>
      {mfsResult && <pre className="preview-box">{mfsResult}</pre>}
    </div>
    <div className="ipns-card">
      <h3>Kubo Binary Verification</h3>
      <button className="btn-small" onClick={async () => { try { setBinaryInfo(JSON.stringify(await invoke("get_binary_verification_info"), null, 2)); } catch (e) { setError(formatError(e)); } }}>Inspect binary</button>
      <div className="input-row"><input className="cid-input" value={binaryHash} onChange={e => setBinaryHashValue(e.target.value)} placeholder="Expected SHA-256 (empty disables pinning)" />
        <button className="btn-small btn-pin" onClick={async () => { try { await invoke("set_binary_hash", { hash: binaryHash.trim() || null }); setError(""); } catch (e) { setError(formatError(e)); } }}>Save hash</button></div>
      {binaryInfo && <pre className="preview-box">{binaryInfo}</pre>}
    </div>
  </div>;
}
