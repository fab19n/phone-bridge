export interface Summary {
  n: number;
  mean: number;
  p50: number;
  p95: number;
  p99: number;
  max: number;
}

/** Nearest-rank percentiles, rounded to 0.01. NaN fields (null in JSON) when `values` is empty. */
export function summarize(values: readonly number[]): Summary {
  if (values.length === 0) {
    return { n: 0, mean: NaN, p50: NaN, p95: NaN, p99: NaN, max: NaN };
  }
  const sorted = [...values].sort((a, b) => a - b);
  const at = (p: number) => sorted[Math.max(0, Math.ceil(p * sorted.length) - 1)];
  const round = (x: number) => Math.round(x * 100) / 100;
  return {
    n: sorted.length,
    mean: round(sorted.reduce((sum, v) => sum + v, 0) / sorted.length),
    p50: round(at(0.5)),
    p95: round(at(0.95)),
    p99: round(at(0.99)),
    max: round(sorted[sorted.length - 1]),
  };
}
