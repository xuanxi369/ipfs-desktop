import { describe, expect, it } from "vitest";
import { mergeDashboardTick } from "./dashboardTick";

describe("dashboard background ticks", () => {
  it("creates an initial dashboard snapshot from a partial tick", () => {
    const merged = mergeDashboardTick(null, { peers: { peers: [] }, bandwidth: null, bitswap: null, repo: null });
    expect(merged.pin_count).toBe(0);
    expect(merged.peers).toEqual({ peers: [] });
    expect(merged.repo).toBeNull();
  });

  it("updates live fields without discarding cached dashboard data", () => {
    const previous = { node_id: { id: "peer", agent_version: "kubo/1" }, version: null, repo: null, peers: null, bandwidth: null, bitswap: null, pin_count: 4 };
    const bandwidth = { total_in: 10, total_out: 20, rate_in: 1, rate_out: 2 };
    const merged = mergeDashboardTick(previous, { bandwidth, peers: null, bitswap: null, repo: null });
    expect(merged.node_id).toEqual({ id: "peer", agent_version: "kubo/1" });
    expect(merged.pin_count).toBe(4);
    expect(merged.bandwidth).toEqual(bandwidth);
  });
});
