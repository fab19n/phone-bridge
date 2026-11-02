// The Stage 1 spike runners (docs/DEVELOPMENT_PLAN.md). Each returns plain JSON for
// docs/STAGE1_RESULTS.md; scripts/run-spikes.mjs calls them through window.__spikes.
import { Channel, invoke } from "@tauri-apps/api/core";
import { nowUs, parseVideoMessage } from "../lib/videoChannel";
import { SpikeDecoder } from "./decoder";
import { summarize } from "./stats";

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** Chromium's non-standard heap size, in MB (null where unavailable). */
function heapMb(): number | null {
  const memory = (performance as Performance & { memory?: { usedJSHeapSize: number } }).memory;
  return memory ? Math.round((memory.usedJSHeapSize / 1048576) * 10) / 10 : null;
}

// --- 0B: which codecs can WebView2 decode?
const CODECS = [
  "avc1.42E01F", // Baseline 3.1
  "avc1.4D401F", // Main 3.1
  "avc1.640028", // High 4.0
  "avc1.640029", // High 4.1
  "avc1.64002A", // High 4.2
  "avc1.640033", // High 5.1
  "hvc1.1.6.L120.90",
  "av01.0.08M.08",
];

export async function runWebCodecsMatrix() {
  if (typeof VideoDecoder === "undefined") return { available: false, results: [] };
  const results = [];
  for (const codec of CODECS) {
    for (const hardwareAcceleration of ["prefer-hardware", "no-preference"] as const) {
      try {
        const support = await VideoDecoder.isConfigSupported({
          codec,
          codedWidth: 1080,
          codedHeight: 1920,
          hardwareAcceleration,
          optimizeForLatency: true,
        });
        results.push({ codec, hardwareAcceleration, supported: support.supported === true });
      } catch (e) {
        results.push({ codec, hardwareAcceleration, supported: false, error: String(e) });
      }
    }
  }
  return { available: true, results };
}

// --- 0C: raw vs JSON channel throughput and latency
export interface ThroughputOptions {
  mode: "raw" | "json";
  fps: number;
  payloadBytes: number;
  seconds: number;
}

type JsonFrame = { seq: number; recvUs: number; data: number[] };

export async function runThroughput(options: ThroughputOptions) {
  const expected = options.fps * options.seconds;
  const latencyMs: number[] = [];
  const gapsMs: number[] = [];
  // Arrival gaps over 50 ms (3 frames at 60 fps), timed from the first heap sample so they line up with
  // the 1-second samples: a stall at the moment the heap drops points at garbage collection.
  const stalls: { atS: number; gapMs: number }[] = [];
  let received = 0;
  let outOfOrder = 0;
  let wrongSize = 0;
  let lastSeq = -1;
  let lastArrival = 0;
  const t0 = performance.now();
  const heap = [heapMb()];
  const sampler = setInterval(() => heap.push(heapMb()), 1000);
  let finish = () => {};
  const allReceived = new Promise<void>((resolve) => (finish = resolve));

  const channel = new Channel<ArrayBuffer | JsonFrame>();
  channel.onmessage = (message) => {
    const arrival = performance.now();
    const now = nowUs();
    let seq: number;
    let recvUs: number;
    let size: number;
    if (message instanceof ArrayBuffer) {
      const parsed = parseVideoMessage(message);
      [seq, recvUs, size] = [parsed.ptsUs, parsed.recvUs, parsed.payload.byteLength];
    } else {
      [seq, recvUs, size] = [message.seq, message.recvUs, message.data.length];
    }
    latencyMs.push((now - recvUs) / 1000);
    if (lastArrival) {
      const gap = arrival - lastArrival;
      gapsMs.push(gap);
      if (gap > 50) stalls.push({ atS: Math.round((lastArrival - t0) / 100) / 10, gapMs: Math.round(gap) });
    }
    lastArrival = arrival;
    if (seq !== lastSeq + 1) outOfOrder++;
    lastSeq = seq;
    if (size !== options.payloadBytes) wrongSize++;
    if (++received === expected) finish();
  };

  const started = performance.now();
  try {
    const rust = await invoke("spike_throughput", { ...options, onFrame: channel });
    await Promise.race([allReceived, sleep(10_000)]);
    const samples = heap.filter((mb): mb is number => mb !== null);
    // The minimum of each 10-sample window is roughly the heap after a garbage collection: if they
    // climb over the run, something leaks.
    const troughs = [];
    for (let i = 0; i < samples.length; i += 10) troughs.push(Math.min(...samples.slice(i, i + 10)));
    return {
      ...options,
      expected,
      received,
      outOfOrder,
      wrongSize,
      wallMs: Math.round(performance.now() - started),
      latencyMs: summarize(latencyMs),
      interArrivalMs: summarize(gapsMs),
      stalls,
      heapMb: {
        start: samples[0] ?? null,
        max: samples.length ? Math.max(...samples) : null,
        end: heapMb(),
        troughs,
        samples,
      },
      rust,
    };
  } finally {
    clearInterval(sampler);
  }
}

// --- 0F: do rapid invokes reach a single writer in order?

/**
 * Keeps one invoke in flight: events that arrive meanwhile go out together in the next call, so they
 * reach Rust in order by construction. The candidate design for Stage 4's control path.
 */
class SerialSender {
  private queue: number[] = [];
  private pumping: Promise<void> | null = null;
  invokes = 0;
  failed = 0;

  push(seq: number): void {
    this.queue.push(seq);
    this.pumping ??= this.pump();
  }

  private async pump(): Promise<void> {
    try {
      while (this.queue.length) {
        const seqs = this.queue.splice(0);
        this.invokes++;
        await invoke("spike_ordering_send_batch", { seqs }).catch(() => this.failed++);
      }
    } finally {
      this.pumping = null;
    }
  }

  async drain(): Promise<void> {
    while (this.pumping) await this.pumping;
  }
}

/** Fire-and-forget: every call starts at once, as the scaffold's pointer handlers did. */
async function sendConcurrently(n: number, variant: "async" | "sync" | "batched") {
  const calls: Promise<unknown>[] = [];
  if (variant === "batched") {
    for (let i = 0; i < n; i += 16) {
      const seqs = Array.from({ length: Math.min(16, n - i) }, (_, k) => i + k);
      calls.push(invoke("spike_ordering_send_batch", { seqs }));
    }
  } else {
    const command = variant === "async" ? "spike_ordering_send_async" : "spike_ordering_send_sync";
    for (let seq = 0; seq < n; seq++) calls.push(invoke(command, { seq }));
  }
  const settled = await Promise.allSettled(calls);
  return { invokes: calls.length, failedInvokes: settled.filter((r) => r.status === "rejected").length };
}

async function sendSerially(n: number, paced: boolean) {
  const sender = new SerialSender();
  for (let seq = 0; seq < n; seq++) {
    sender.push(seq);
    if (paced) await sleep(4);
  }
  await sender.drain();
  return { invokes: sender.invokes, failedInvokes: sender.failed };
}

export async function runOrdering(count = 1000) {
  const variants = [];
  for (const variant of ["async", "sync", "batched", "serial", "serialPaced"] as const) {
    // The paced run sends one event every ~4 ms (faster than a 120 Hz mouse), so it uses fewer events.
    const n = variant === "serialPaced" ? Math.min(count, 300) : count;
    await invoke("spike_ordering_start");
    const started = performance.now();
    const { invokes, failedInvokes } =
      variant === "serial" || variant === "serialPaced"
        ? await sendSerially(n, variant === "serialPaced")
        : await sendConcurrently(n, variant);
    const totalMs = Math.round(performance.now() - started);
    const stats = await invoke<Record<string, unknown>>("spike_ordering_finish", { expected: n });
    variants.push({ variant, events: n, invokes, failedInvokes, totalMs, ...stats });
  }
  return { count, variants };
}

// --- 0G: the adb server, over TCP 5037 only
export const runAdbProbe = (trackMs = 2000) => invoke("spike_adb_probe", { trackMs });

// --- 0A/0B/0D: the full pipeline into WebCodecs, from a fixture or a phone
async function streamThrough<T>(
  canvas: HTMLCanvasElement,
  command: string,
  args: Record<string, unknown>,
  messagesSent: (rust: T) => number,
) {
  const decoder = new SpikeDecoder(canvas);
  const channel = new Channel<ArrayBuffer>();
  channel.onmessage = (message) => decoder.handle(message);
  try {
    const rust = await invoke<T>(command, { ...args, onVideo: channel });
    // Messages over 1 KiB are fetched asynchronously; wait for the ones still in flight.
    const deadline = performance.now() + 5000;
    while (decoder.messages < messagesSent(rust) && performance.now() < deadline) await sleep(20);
    await decoder.flush();
    return { rust, page: decoder.report() };
  } finally {
    decoder.close();
  }
}

export const runReplay = (
  canvas: HTMLCanvasElement,
  { fixture = "synthetic-rotation", realtime = false }: { fixture?: string; realtime?: boolean } = {},
) =>
  streamThrough<{ messagesSent: number }>(canvas, "spike_replay", { fixture, realtime }, (r) => r.messagesSent);

export interface MirrorOptions {
  serial?: string;
  seconds?: number;
  maxSize?: number;
  videoBitRate?: number;
  /** Record to fixtures/local/<name>.bin. */
  recordAs?: string;
}

export const runMirror = (canvas: HTMLCanvasElement, options: MirrorOptions = {}) =>
  streamThrough<{ stream: { messagesSent: number } }>(
    canvas,
    "spike_mirror",
    {
      options: {
        serial: options.serial ?? null,
        seconds: options.seconds ?? 10,
        maxSize: options.maxSize ?? 1920,
        videoBitRate: options.videoBitRate ?? 8_000_000,
        recordAs: options.recordAs ?? null,
      },
    },
    (r) => r.stream.messagesSent,
  );

/** Every spike that runs without a phone, in order. Takes about 90 s. */
export async function runAll(canvas: HTMLCanvasElement) {
  const results: Record<string, unknown> = { started: new Date().toISOString(), userAgent: navigator.userAgent };
  const step = async (name: string, run: () => Promise<unknown>) => {
    try {
      results[name] = await run();
    } catch (e) {
      results[name] = { error: String(e) };
    }
  };
  await step("webcodecs", runWebCodecsMatrix);
  await step("replay", () => runReplay(canvas));
  await step("replayRealtime", () => runReplay(canvas, { realtime: true }));
  await step("throughputRaw", () => runThroughput({ mode: "raw", fps: 60, payloadBytes: 50_000, seconds: 60 }));
  await step("throughputRawBurst", () => runThroughput({ mode: "raw", fps: 60, payloadBytes: 200_000, seconds: 10 }));
  await step("throughputJson", () => runThroughput({ mode: "json", fps: 60, payloadBytes: 50_000, seconds: 5 }));
  await step("ordering", () => runOrdering(1000));
  await step("adb", () => runAdbProbe(2000));
  results.finished = new Date().toISOString();
  return results;
}

/** What the page exposes as window.__spikes, for scripts/run-spikes.mjs. */
export function createSpikeApi(canvas: () => HTMLCanvasElement) {
  return {
    runAll: () => runAll(canvas()),
    webcodecs: runWebCodecsMatrix,
    replay: (options?: { fixture?: string; realtime?: boolean }) => runReplay(canvas(), options),
    mirror: (options?: MirrorOptions) => runMirror(canvas(), options),
    throughput: runThroughput,
    ordering: runOrdering,
    adb: runAdbProbe,
  };
}

declare global {
  interface Window {
    __spikes?: ReturnType<typeof createSpikeApi>;
  }
}
