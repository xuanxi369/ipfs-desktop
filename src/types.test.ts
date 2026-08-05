import { describe, expect, it } from "vitest";
import { formatBytes, formatError } from "./types";

describe("shared UI formatters", () => {
  it("formats structured backend errors", () => {
    expect(formatError({ Backend: { kind: "Unsupported", message: "not available" } }))
      .toBe("Backend Unsupported: not available");
  });

  it("formats byte values", () => {
    expect(formatBytes(1024)).toBe("1 KB");
  });
});
