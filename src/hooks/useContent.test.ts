import { act, renderHook, waitFor } from "@testing-library/react";
import type { TFunction } from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useContent } from "./useContent";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open, save: mocks.save }));
const t = ((key: string) => key) as TFunction;

describe("useContent", () => {
  beforeEach(() => { vi.clearAllMocks(); });

  it("uploads every selected file and refreshes the content index", async () => {
    mocks.open.mockResolvedValue(["a.txt", "b.txt"]);
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, string>) => {
      if (command === "add_file_with_progress") return { name: args?.filePath, hash: `cid-${args?.filePath}`, size: 1 };
      if (command === "list_content") return [{ cid: "cid-a.txt", name: "a.txt", size: 1, added_at: 1 }];
    });
    const { result } = renderHook(() => useContent(vi.fn(), t));
    await act(() => result.current.selectAndUpload());
    expect(result.current.uploads).toHaveLength(2);
    expect(result.current.contentRecords).toHaveLength(1);
  });

  it("downloads the trimmed CID to the selected path", async () => {
    mocks.save.mockResolvedValue("D:\\download.bin");
    mocks.invoke.mockResolvedValue(null);
    const { result } = renderHook(() => useContent(vi.fn(), t));
    act(() => result.current.setDownloadCid("  bafy-test-cid  "));
    await act(() => result.current.downloadByCid());
    expect(mocks.invoke).toHaveBeenCalledWith("download_file", { cid: "bafy-test-cid", savePath: "D:\\download.bin" });
    expect(result.current.downloading).toBe(false);
  });

  it("tracks download and upload progress payloads", () => {
    const { result } = renderHook(() => useContent(vi.fn(), t));
    act(() => {
      result.current.setDownloadProgress({ cid: "cid", loaded: 50, total: 100 });
      result.current.setUploadProgress({ name: "a", loaded: 25, total: 100 });
    });
    expect(result.current.downloadProgress?.loaded).toBe(50);
    expect(result.current.uploadProgress?.loaded).toBe(25);
  });

  it("adds and removes pins then refreshes the list", async () => {
    mocks.invoke.mockImplementation(async (command: string) => command === "get_pin_list" ? { pins: [{ cid: "bafy-pin", type: "recursive" }] } : null);
    const { result } = renderHook(() => useContent(vi.fn(), t));
    act(() => result.current.setPinCid("bafy-pin"));
    await act(() => result.current.addPinByCid());
    expect(mocks.invoke).toHaveBeenCalledWith("add_pin", { cid: "bafy-pin" });
    expect(result.current.pinList).toHaveLength(1);
    await act(() => result.current.removePinByCid("bafy-pin"));
    expect(mocks.invoke).toHaveBeenCalledWith("remove_pin", { cid: "bafy-pin" });
  });

  it("debounces backend route lookup", async () => {
    mocks.invoke.mockResolvedValue("iroh");
    const { result } = renderHook(() => useContent(vi.fn(), t));
    act(() => result.current.setDownloadCid("bafy-route"));
    await waitFor(() => expect(result.current.routeHint).toBe("iroh"), { timeout: 1000 });
    expect(mocks.invoke).toHaveBeenCalledWith("get_backend_route", { cid: "bafy-route" });
  });
});
