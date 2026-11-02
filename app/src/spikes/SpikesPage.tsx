import { useEffect, useRef, useState } from "react";
import { createSpikeApi } from "./runners";

type SpikeApi = ReturnType<typeof createSpikeApi>;

const SPIKES: [string, (api: SpikeApi) => Promise<unknown>][] = [
  ["0B WebCodecs support", (api) => api.webcodecs()],
  ["0B/0D replay fixture", (api) => api.replay()],
  ["0C raw channel, 60 s", (api) => api.throughput({ mode: "raw", fps: 60, payloadBytes: 50_000, seconds: 60 })],
  ["0C JSON channel, 5 s", (api) => api.throughput({ mode: "json", fps: 60, payloadBytes: 50_000, seconds: 5 })],
  ["0F control ordering", (api) => api.ordering(1000)],
  ["0G adb probe", (api) => api.adb(2000)],
  ["0A/0D mirror phone, 10 s", (api) => api.mirror({ seconds: 10 })],
  ["All phone-free spikes", (api) => api.runAll()],
];

/** The Stage 1 spikes page (`npm run tauri:spikes`). Test IDs refer to docs/DEVELOPMENT_PLAN.md. */
export default function SpikesPage() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [running, setRunning] = useState<string | null>(null);
  const [output, setOutput] = useState("");

  // One API instance, shared by the buttons and by scripts/run-spikes.mjs.
  useEffect(() => {
    window.__spikes = createSpikeApi(() => {
      if (!canvasRef.current) throw new Error("canvas not mounted");
      return canvasRef.current;
    });
    return () => {
      delete window.__spikes;
    };
  }, []);

  async function run(name: string, spike: (api: SpikeApi) => Promise<unknown>) {
    const api = window.__spikes;
    if (!api) return;
    setRunning(name);
    setOutput(`${name}: running…`);
    try {
      setOutput(JSON.stringify(await spike(api), null, 2));
    } catch (e) {
      setOutput(`${name} failed: ${e}`);
    } finally {
      setRunning(null);
    }
  }

  return (
    <main style={{ padding: 16, display: "grid", gap: 12 }}>
      <h1 style={{ fontSize: 18, margin: 0 }}>PhoneBridge · Stage 1 spikes</h1>
      <p style={{ margin: 0, color: "var(--muted)", fontSize: 13 }}>
        Test IDs are from docs/DEVELOPMENT_PLAN.md. scripts/run-spikes.mjs runs the phone-free set unattended.
      </p>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
        {SPIKES.map(([name, spike]) => (
          <button key={name} disabled={running !== null} onClick={() => run(name, spike)}>
            {name}
          </button>
        ))}
      </div>
      <canvas
        ref={canvasRef}
        width={480}
        height={1040}
        style={{ maxWidth: "100%", maxHeight: 420, background: "#000", justifySelf: "start" }}
      />
      <pre style={{ fontSize: 12, whiteSpace: "pre-wrap", margin: 0 }}>{output}</pre>
    </main>
  );
}
