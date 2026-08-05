import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import AdvancedTools from "./AdvancedTools";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

const config = { ipfs_path: null, api_addr: "http://127.0.0.1:5001", gateway_addr: "http://127.0.0.1:8080", daemon_flags: [], auto_launch: false, auto_gc: true, auto_restart: true, route_policy: "KuboOnly" as const, kubo_binary_sha256: null };

describe("AdvancedTools", () => {
  beforeEach(() => mocks.invoke.mockReset());
  afterEach(() => cleanup());

  it("invokes MFS read for the selected path", async () => {
    mocks.invoke.mockResolvedValueOnce(Array.from(new TextEncoder().encode("hello")));
    render(<AdvancedTools isRunning setError={vi.fn()} config={config} onConfigSaved={vi.fn()} />);
    fireEvent.change(screen.getByPlaceholderText("/path"), { target: { value: "/notes.txt" } });
    fireEvent.click(screen.getByText("Read"));
    expect(mocks.invoke).toHaveBeenCalledWith("mfs_read", { path: "/notes.txt" });
  });

  it("saves edited API and gateway addresses", async () => {
    mocks.invoke.mockResolvedValueOnce(undefined);
    const saved = vi.fn();
    render(<AdvancedTools isRunning setError={vi.fn()} config={config} onConfigSaved={saved} />);
    fireEvent.change(screen.getByLabelText("API address"), { target: { value: "https://node.example:5001" } });
    fireEvent.click(screen.getByText("Save endpoints"));
    expect(mocks.invoke).toHaveBeenCalledWith("update_config", { newConfig: expect.objectContaining({ api_addr: "https://node.example:5001" }) });
  });

  it("keeps MFS controls interactive while the daemon is offline", () => {
    mocks.invoke.mockResolvedValueOnce([]);
    render(<AdvancedTools isRunning={false} setError={vi.fn()} config={config} onConfigSaved={vi.fn()} />);
    const path = screen.getByPlaceholderText("/path");
    fireEvent.change(path, { target: { value: "/offline-draft" } });
    expect((path as HTMLInputElement).value).toBe("/offline-draft");
    fireEvent.click(screen.getByText("List"));
    expect(mocks.invoke).toHaveBeenCalledWith("mfs_ls", { path: "/offline-draft" });
  });
});
