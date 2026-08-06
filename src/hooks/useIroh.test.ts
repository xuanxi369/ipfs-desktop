import { act, renderHook } from "@testing-library/react";
import type { TFunction } from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useIroh } from "./useIroh";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open, save: mocks.save }));
const t = ((key: string) => key) as TFunction;

describe("useIroh", () => {
  beforeEach(() => vi.clearAllMocks());

  it("loads usage mode, migration status, and node information", async () => {
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "get_usage_mode") return "Compatible";
      if (command === "get_migration_status") return { progress_percent: 75 };
      return { peer_id: "peer-a", agent_version: "iroh/1" };
    });
    const { result } = renderHook(() => useIroh(vi.fn(), t));
    await act(() => Promise.all([result.current.loadRoutePolicy(), result.current.loadIrohInfo()]));
    expect(result.current.routePolicy).toBe("Compatible");
    expect(result.current.migrationStatus?.progress_percent).toBe(75);
    expect(result.current.irohInfo?.peer_id).toBe("peer-a");
  });

  it("adds, shares, and registers iroh content", async () => {
    mocks.open.mockResolvedValue("D:\\file.bin");
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "iroh_add_file") return { cid: "hash-a" };
      if (command === "iroh_share") return "blob-ticket";
      if (command === "iroh_register_ticket") return "hash-remote";
    });
    const { result } = renderHook(() => useIroh(vi.fn(), t));
    await act(() => result.current.irohAddFile());
    await act(() => result.current.irohShare());
    act(() => result.current.setIrohFetchInput("remote-ticket"));
    await act(() => result.current.irohRegisterTicket());
    expect(result.current.irohTicket).toBe("blob-ticket");
    expect(result.current.irohFetchResult).toContain("hash-remote");
  });

  it("reports unsupported backend errors and resets busy state", async () => {
    mocks.open.mockResolvedValue("D:\\file.bin");
    mocks.invoke.mockRejectedValue("enable iroh-backend feature");
    const setError = vi.fn();
    const { result } = renderHook(() => useIroh(setError, t));
    await act(() => result.current.irohAddFile());
    expect(setError).toHaveBeenCalledWith(expect.stringContaining("iroh-backend"));
    expect(result.current.irohBusy).toBe(false);
    expect(result.current.irohInfo).toBeNull();
  });
});
