const KEY = "ipfs-benchmark-history";
export type BenchmarkSnapshot = { timestamp: string; winner?: string; speedup_ratio?: number; total_duration_ms?: number };

export function loadBenchmarkHistory(): BenchmarkSnapshot[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) || "[]");
    return Array.isArray(parsed)
      ? parsed.filter((item): item is BenchmarkSnapshot =>
          typeof item === "object" && item !== null && typeof item.timestamp === "string")
      : [];
  } catch {
    return [];
  }
}

export function recordBenchmark(result: Record<string, unknown>): BenchmarkSnapshot[] {
  const current = loadBenchmarkHistory();
  const item = { timestamp: String(result.timestamp || new Date().toISOString()), winner: result.winner as string | undefined, speedup_ratio: result.speedup_ratio as number | undefined, total_duration_ms: result.total_duration_ms as number | undefined };
  const next = [item, ...current].slice(0, 20);
  localStorage.setItem(KEY, JSON.stringify(next));
  return next;
}
