import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useIpns } from "./useIpns";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

describe("useIpns", () => {
  beforeEach(() => vi.clearAllMocks());

  it("publishes and resolves IPNS names", async () => {
    mocks.invoke.mockImplementation(async (command: string) => command === "ipns_publish" ? { Name: "k51-name", Value: "/ipfs/bafy-cid" } : { Path: "/ipfs/bafy-cid" });
    const { result } = renderHook(() => useIpns(vi.fn()));
    act(() => { result.current.setIpnsCid("bafy-cid"); result.current.setIpnsKeyName("site"); result.current.setIpnsResolveName("k51-name"); });
    await act(() => result.current.publishIpns());
    await act(() => result.current.resolveIpns());
    expect(mocks.invoke).toHaveBeenCalledWith("ipns_publish", { cid: "bafy-cid", keyName: "site", lifetime: "24h" });
    expect(result.current.ipnsPublishResult).toContain("k51-name");
    expect(result.current.ipnsResolveResult).toBe("/ipfs/bafy-cid");
  });

  it("generates and deletes keys while refreshing the key list", async () => {
    const keys = [{ public_key: "pub", ipns_name: "k51", label: "blog" }];
    mocks.invoke.mockImplementation(async (command: string) => command === "list_keys" ? keys : null);
    const { result } = renderHook(() => useIpns(vi.fn()));
    act(() => result.current.setNewKeyLabel("blog"));
    await act(() => result.current.generateNewKey());
    expect(mocks.invoke).toHaveBeenCalledWith("generate_key", { label: "blog" });
    expect(result.current.keyList).toEqual(keys);
    await act(() => result.current.deleteKeyByLabel("blog"));
    expect(mocks.invoke).toHaveBeenCalledWith("delete_key", { label: "blog" });
  });

  it("reports publish errors without clearing the input", async () => {
    mocks.invoke.mockRejectedValue("daemon offline");
    const setError = vi.fn();
    const { result } = renderHook(() => useIpns(setError));
    act(() => result.current.setIpnsCid("bafy-cid"));
    await act(() => result.current.publishIpns());
    expect(setError).toHaveBeenCalledWith(expect.stringContaining("daemon offline"));
    expect(result.current.ipnsCid).toBe("bafy-cid");
  });
});
