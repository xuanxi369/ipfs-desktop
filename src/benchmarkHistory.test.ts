import { beforeEach, describe, expect, it } from "vitest";
import { loadBenchmarkHistory, recordBenchmark } from "./benchmarkHistory";

describe("benchmark history", () => {
  beforeEach(() => localStorage.clear());
  it("records newest result first", () => {
    recordBenchmark({ timestamp: "2026-01-01T00:00:00Z", winner: "iroh", speedup_ratio: 2 });
    expect(loadBenchmarkHistory()[0].winner).toBe("iroh");
  });
  it("ignores valid JSON that is not an array", () => {
    localStorage.setItem("ipfs-benchmark-history", "null");
    expect(loadBenchmarkHistory()).toEqual([]);
  });
  it("filters malformed entries", () => {
    localStorage.setItem("ipfs-benchmark-history", JSON.stringify([null, {}, { timestamp: "2026-01-01T00:00:00Z" }]));
    expect(loadBenchmarkHistory()).toEqual([{ timestamp: "2026-01-01T00:00:00Z" }]);
  });
});
