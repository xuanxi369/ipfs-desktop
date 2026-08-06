import type { DashboardStats, DashboardTick } from "./types";

export function mergeDashboardTick(previous: DashboardStats | null, tick: DashboardTick): DashboardStats {
  if (previous) return { ...previous, ...tick };
  return {
    node_id: null,
    version: null,
    repo: tick.repo || null,
    peers: tick.peers || null,
    bandwidth: tick.bandwidth || null,
    bitswap: tick.bitswap || null,
    pin_count: 0,
  };
}
