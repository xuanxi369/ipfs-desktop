import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppConfig, DaemonStatus } from "../types";
import { formatError } from "../types";

export function useDaemon(setError: (message: string) => void) {
  const [status, setStatus] = useState<DaemonStatus>({ type: "Stopped" });
  const [config, setConfig] = useState<AppConfig | null>(null);

  async function loadConfig() {
    try { setConfig(await invoke<AppConfig>("get_config")); setError(""); }
    catch (e) { setError(`Failed to load config: ${formatError(e)}`); }
  }
  async function loadStatus() {
    try { setStatus(await invoke<DaemonStatus>("get_daemon_status")); setError(""); }
    catch (e) { setError(`Failed to get status: ${formatError(e)}`); }
  }
  async function run(command: "start_daemon" | "stop_daemon" | "restart_daemon", label: string) {
    try { await invoke(command); await loadStatus(); setError(""); }
    catch (e) { setError(`Failed to ${label}: ${formatError(e)}`); }
  }
  async function openWebui() {
    try { await invoke("open_webui"); setError(""); }
    catch (e) { setError(`Failed to open WebUI: ${formatError(e)}`); }
  }
  async function toggleAutoLaunch() {
    try { await invoke("set_auto_launch", { enable: !config?.auto_launch }); await loadConfig(); }
    catch (e) { setError(`Auto-launch error: ${formatError(e)}`); }
  }

  useEffect(() => {
    let mounted = true;
    const unlisten = listen<DaemonStatus>("daemon-status-changed", ({ payload }) => {
      if (mounted) setStatus(payload);
    });
    void loadConfig();
    void (async () => {
      try {
        const current = await invoke<DaemonStatus>("get_daemon_status");
        if (!mounted) return;
        setStatus(current);
      } catch (e) {
        if (mounted) setError(`Failed to inspect Kubo status: ${formatError(e)}`);
      }
    })();
    return () => { mounted = false; void unlisten.then((fn) => fn()); };
  }, []);

  return { status, config, setConfig, loadConfig, loadStatus,
    startDaemon: () => run("start_daemon", "start"), stopDaemon: () => run("stop_daemon", "stop"),
    restartDaemon: () => run("restart_daemon", "restart"), openWebui, toggleAutoLaunch };
}
