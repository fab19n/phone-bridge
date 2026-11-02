import { describe, expect, it } from "vitest";
import { summarize } from "./stats";

describe("summarize", () => {
  it("uses nearest-rank percentiles", () => {
    const values = Array.from({ length: 100 }, (_, i) => 100 - i); // 100..1, unsorted on purpose
    expect(summarize(values)).toEqual({ n: 100, mean: 50.5, p50: 50, p95: 95, p99: 99, max: 100 });
  });

  it("handles a single value and an empty list", () => {
    expect(summarize([3.14159]).p99).toBe(3.14);
    expect(summarize([]).n).toBe(0);
  });
});
