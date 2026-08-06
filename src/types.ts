// ── 共享类型定义和工具函数 ──
// 从 App.tsx 提取，供所有标签页子组件使用

// ═══════════════════════════════════════════════
// 类型定义
// ═══════════════════════════════════════════════

export interface DaemonStatus {
  type: string;
  data?: {
    pid?: number;
    peer_id?: string;
    api_addr?: string;
    error?: string;
  };
}

export interface AppConfig {
  ipfs_path: string | null;
  api_addr: string;
  gateway_addr: string;
  allow_remote_api: boolean;
  daemon_flags: string[];
  auto_launch: boolean;
  auto_gc: boolean;
  auto_restart: boolean;
  route_policy: "KuboOnly" | "IrohOnly" | "Auto" | "Mirror";
  usage_mode?: "LocalFirst" | "Compatible" | "Mirrored" | null;
  kubo_binary_sha256: string | null;
}

export interface AddResult {
  Hash: string;
  Size: string;
  Name: string;
}
export interface ContentRecord { cid: string; name: string; size: number; backend: string; added_at: number; }

export interface StructuredError {
  BinaryNotFound?: null;
  BinaryVerificationFailed?: string;
  ProcessStartFailed?: string;
  ProcessExitedUnexpectedly?: null;
  ProcessStopFailed?: string;
  InvalidState?: null;
  ApiError?: string;
  ApiConnectionFailed?: { addr: string; detail: string };
  ApiParseError?: string;
  ConfigError?: string;
  IoError?: string;
  Backend?: { kind: string; message: string };
}

export interface PinEntry {
  Cid: string;
  Type: string;
}

export interface PinList {
  pins: PinEntry[];
}

export interface DashboardStats {
  node_id: { id: string; agent_version: string } | null;
  version: string | null;
  repo: { num_objects?: number; repo_size?: number } | null;
  peers: { peers: { peer: string; addr: string }[] } | null;
  bandwidth: { total_in: number; total_out: number; rate_in: number; rate_out: number } | null;
  bitswap: { blocks_received: number; blocks_sent: number; data_received: number; data_sent: number } | null;
  pin_count: number;
}

export interface DownloadProgress {
  cid: string;
  loaded: number;
  total: number | null;
}

export interface UploadProgress {
  name: string;
  loaded: number;
  total: number;
}

export interface NodeIdentityInfo {
  label: string;
  created_at: number;
  kubo_peer_id: string | null;
  iroh_node_id: string | null;
}

export interface NodeHealth {
  app_uptime_secs: number;
  daemon_uptime_secs: number | null;
  kubo_running: boolean;
  num_objects: number | null;
  repo_size: number | null;
  peers: number | null;
  bytes_in: number | null;
  bytes_out: number | null;
  iroh_content_count: number | null;
}

export type TabName = "dashboard" | "webui" | "files" | "pins" | "ipns" | "iroh" | "advanced";

export interface DashboardTick {
  peers: { peers: { peer: string; addr: string }[] } | null;
  bandwidth: { total_in: number; total_out: number; rate_in: number; rate_out: number } | null;
  bitswap: { blocks_received: number; blocks_sent: number; data_received: number; data_sent: number } | null;
  repo: { num_objects?: number; repo_size?: number } | null;
}

// ═══════════════════════════════════════════════
// 工具函数
// ═══════════════════════════════════════════════

export function formatError(e: unknown): string {
  if (typeof e === "string") return e;
  if (typeof e === "object" && e !== null) {
    const err = e as StructuredError;
    if (err.BinaryNotFound !== undefined) return "IPFS binary not found. Please install Kubo.";
    if (err.BinaryVerificationFailed) return `Binary verification failed: ${err.BinaryVerificationFailed}`;
    if (err.ProcessStartFailed) return `Process start failed: ${err.ProcessStartFailed}`;
    if (err.ProcessExitedUnexpectedly !== undefined) return "Daemon process exited unexpectedly.";
    if (err.ProcessStopFailed) return `Process stop failed: ${err.ProcessStopFailed}`;
    if (err.InvalidState !== undefined) return "Invalid daemon state for this operation.";
    if (err.ApiError) return `API error: ${err.ApiError}`;
    if (err.ApiConnectionFailed) return `API connection failed at ${err.ApiConnectionFailed.addr}: ${err.ApiConnectionFailed.detail}`;
    if (err.ApiParseError) return `API parse error: ${err.ApiParseError}`;
    if (err.ConfigError) return `Configuration error: ${err.ConfigError}`;
    if (err.IoError) return `I/O error: ${err.IoError}`;
    if (err.Backend) return `Backend ${err.Backend.kind}: ${err.Backend.message}`;
    return JSON.stringify(e);
  }
  return String(e);
}

export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

export function formatRate(bytesPerSec: number): string {
  return formatBytes(bytesPerSec) + "/s";
}

export function formatUptime(secs: number): string {
  if (secs <= 0) return "0m";
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const parts: string[] = [];
  if (d) parts.push(`${d}d`);
  if (h) parts.push(`${h}h`);
  parts.push(`${m}m`);
  return parts.join(" ");
}

export function shortHash(value: string, size = 12): string {
  if (value.length <= size * 2 + 1) return value;
  return `${value.slice(0, size)}…${value.slice(-size)}`;
}
