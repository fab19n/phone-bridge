// Runs the Stage 1 spikes unattended (docs/DEVELOPMENT_PLAN.md): launches the spikes build with WebView2
// remote debugging, calls window.__spikes over the Chrome DevTools Protocol, and prints or saves the JSON.
//
//   node scripts/run-spikes.mjs [--build] [--only replay,adb,...] [--out results.json]
//
// --build    run `npm run spikes:build` first (a release build with the `spikes` feature)
// --only     run only these spikes (webcodecs, replay, replayRealtime, throughputRaw, throughputRawBurst,
//            throughputJson, ordering, adb, mirror); default: runAll(), every phone-free spike
// --out      also write the results to this file
//
// The window has to stay visible while it runs: WebView2 throttles timers in hidden windows.
import { execFileSync, execSync, spawn } from "node:child_process";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const exe = join(repoRoot, "target", "release", "phonebridge.exe");
const PORT = 9333;

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const value = (name) => (args.includes(name) ? args[args.indexOf(name) + 1] : undefined);
const only = value("--only")?.split(",").filter(Boolean);
const out = value("--out");

const CALLS = {
  webcodecs: "window.__spikes.webcodecs()",
  replay: "window.__spikes.replay()",
  replayRealtime: "window.__spikes.replay({ realtime: true })",
  throughputRaw: "window.__spikes.throughput({ mode: 'raw', fps: 60, payloadBytes: 50000, seconds: 60 })",
  throughputRawBurst: "window.__spikes.throughput({ mode: 'raw', fps: 60, payloadBytes: 200000, seconds: 10 })",
  throughputJson: "window.__spikes.throughput({ mode: 'json', fps: 60, payloadBytes: 50000, seconds: 5 })",
  ordering: "window.__spikes.ordering(1000)",
  adb: "window.__spikes.adb(2000)",
  mirror: "window.__spikes.mirror({ seconds: 10 })",
};
for (const name of only ?? []) {
  if (!(name in CALLS)) throw new Error(`unknown spike ${name}; known: ${Object.keys(CALLS).join(", ")}`);
}

if (flag("--build")) {
  execSync("npm run spikes:build", { cwd: join(repoRoot, "app"), stdio: "inherit" });
}

/** On AC or on battery? Latency on a laptop depends on it, so every result records it. */
function powerSource() {
  try {
    const online = execFileSync(
      "powershell",
      ["-NoProfile", "-Command", "(Get-CimInstance -Namespace root/wmi -ClassName BatteryStatus | Select-Object -First 1).PowerOnline"],
      { encoding: "utf8" },
    ).trim();
    return online === "True" ? "ac" : online === "False" ? "battery" : "no battery reported";
  } catch {
    return "unknown";
  }
}

/** adb.exe process ids, so every result shows whether the user's adb server survived the run (0G). */
function adbPids() {
  try {
    const csv = execFileSync("tasklist", ["/FI", "IMAGENAME eq adb.exe", "/FO", "CSV", "/NH"], { encoding: "utf8" });
    return [...csv.matchAll(/^"adb\.exe","(\d+)"/gm)].map((m) => Number(m[1])).sort((a, b) => a - b);
  } catch {
    return [];
  }
}

/** The result keys a complete run must have. */
const EXPECTED = only ?? [
  "webcodecs", "replay", "replayRealtime", "throughputRaw", "throughputRawBurst", "throughputJson", "ordering", "adb",
];
/** Generous per-call limit: the longest spike (raw throughput) takes about 70 s. */
const CALL_TIMEOUT_MS = 10 * 60 * 1000;

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const power = powerSource();
const adbPidsBefore = adbPids();
const app = spawn(exe, [], {
  env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
  stdio: "ignore",
});
const stop = () => {
  try {
    execFileSync("taskkill", ["/PID", String(app.pid), "/T", "/F"], { stdio: "ignore" });
  } catch {}
};
process.on("SIGINT", () => {
  stop();
  process.exit(130);
});

let exitCode = 0;
try {
  let target;
  for (let i = 0; i < 120 && !target; i++) {
    if (app.exitCode !== null) throw new Error(`${exe} exited with code ${app.exitCode}`);
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      target = list.find((t) => t.type === "page" && !t.url.startsWith("devtools://"));
    } catch {}
    if (!target) await delay(500);
  }
  if (!target) throw new Error("no WebView2 page; is this a spikes build (npm run spikes:build)?");

  const ws = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = reject;
  });
  let nextId = 0;
  const pending = new Map();
  const problems = [];
  // A crash (release builds abort on panic) or a lost connection must fail the pending calls, not hang.
  const failAll = (reason) => {
    for (const { reject } of pending.values()) reject(new Error(reason));
    pending.clear();
  };
  ws.onclose = () => failAll("the DevTools connection closed");
  app.on("exit", (code) => failAll(`the app exited with code ${code}`));
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(`CDP ${msg.error.code}: ${msg.error.message}`));
      else resolve(msg);
    } else if (msg.method === "Runtime.exceptionThrown") {
      problems.push(msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text);
    } else if (msg.method === "Log.entryAdded" && msg.params.entry.level === "error") {
      problems.push(`${msg.params.entry.text}${msg.params.entry.url ? ` <${msg.params.entry.url}>` : ""}`);
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++nextId;
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error(`${method} timed out after ${CALL_TIMEOUT_MS / 1000} s`));
      }, CALL_TIMEOUT_MS);
      const settle = (fn) => (value) => {
        clearTimeout(timer);
        fn(value);
      };
      pending.set(id, { resolve: settle(resolve), reject: settle(reject) });
      ws.send(JSON.stringify({ id, method, params }));
    });
  const evaluate = async (expression) => {
    const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) {
      throw new Error(r.result.exceptionDetails.exception?.description ?? r.result.exceptionDetails.text);
    }
    return r.result?.result?.value;
  };
  await send("Runtime.enable");
  await send("Log.enable");

  for (let i = 0; i < 60 && !(await evaluate("typeof window.__spikes === 'object'")); i++) await delay(500);
  const results = { exe, power, build: await evaluate("navigator.userAgent") };
  if (only) {
    for (const name of only) {
      process.stderr.write(`running ${name}…\n`);
      results[name] = await evaluate(CALLS[name]).catch((e) => ({ error: String(e) }));
    }
  } else {
    process.stderr.write("running every phone-free spike (about 90 s)…\n");
    Object.assign(results, await evaluate("window.__spikes.runAll()"));
  }
  results.pageErrors = problems;
  ws.close();
  stop();
  const adbPidsAfter = adbPids();
  results.adbServer = {
    pidsBefore: adbPidsBefore,
    pidsAfter: adbPidsAfter,
    untouched: adbPidsBefore.length > 0 && adbPidsBefore.join() === adbPidsAfter.join(),
  };

  // Never overwrite a results file with an incomplete run.
  const failed = EXPECTED.filter((name) => !results[name] || results[name].error);
  const json = JSON.stringify(results, null, 2);
  if (failed.length) {
    console.error(`incomplete run, missing or failed: ${failed.join(", ")}${out ? `; not writing ${out}` : ""}`);
    exitCode = 1;
  } else if (out) {
    writeFileSync(out, json + "\n");
  }
  console.log(json);
} catch (e) {
  console.error(String(e));
  exitCode = 1;
} finally {
  stop();
}
process.exit(exitCode);
