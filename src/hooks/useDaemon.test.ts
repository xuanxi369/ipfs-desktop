import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useDaemon } from "./useDaemon";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));

const config = { ipfs_path: null, api_addr: "http://127.0.0.1:5001", gateway_addr: "http://127.0.0.1:8080", allow_remote_api: false, daemon_flags: [], auto_launch: false, auto_gc: true, auto_restart: true, route_policy: "Auto", usage_mode: "Compatible", kubo_binary_sha256: null };

describe("useDaemon", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listen.mockResolvedValue(vi.fn());
  });

  it("keeps Kubo stopped until a compatibility operation needs it", async () => {
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "get_config") return config;
      if (command === "get_daemon_status") return { type: "Stopped" };
      throw new Error(command);
    });
    const setError = vi.fn();
    const { result } = renderHook(() => useDaemon(setError));
    await waitFor(() => expect(result.current.status.type).toBe("Stopped"));
    expect(mocks.invoke).not.toHaveBeenCalledWith("start_daemon");
  });

  it("accepts daemon status events", async () => {
    let handler: ((event: { payload: unknown }) => void) | undefined;
    mocks.listen.mockImplementation(async (_name: string, callback: typeof handler) => { handler = callback; return vi.fn(); });
    mocks.invoke.mockImplementation(async (command: string) => command === "get_config" ? config : { type: "Running", pid: 1, peer_id: "a", api_addr: config.api_addr });
    const { result } = renderHook(() => useDaemon(vi.fn()));
    await waitFor(() => expect(handler).toBeDefined());
    act(() => handler?.({ payload: { type: "Failed", error: "crashed" } }));
    expect(result.current.status).toEqual({ type: "Failed", error: "crashed" });
  });

  it("toggles auto launch and reloads config", async () => {
    mocks.invoke.mockImplementation(async (command: string) => command === "get_config" ? config : command === "get_daemon_status" ? { type: "Running" } : null);
    const { result } = renderHook(() => useDaemon(vi.fn()));
    await waitFor(() => expect(result.current.config).toEqual(config));
    await act(() => result.current.toggleAutoLaunch());
    expect(mocks.invoke).toHaveBeenCalledWith("set_auto_launch", { enable: true });
  });
});
