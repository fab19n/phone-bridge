// Runs the Stage 1 spikes unattended (docs/DEVELOPMENT_PLAN.md): launches the spikes build with WebView2
// remote debugging, calls window.__spikes over the Chrome DevTools Protocol, and prints or saves the JSON.
//
//   node scripts/run-spikes.mjs [--build] [--only replay,adb,...] [--out results.json]
//
// --build    run `npm run spikes:build` first (a release build with the `spikes` feature)
// --only     run only these spikes (webcodecs, replay, replayRealtime, throughputRaw, throughputRawBurst,
//            throughputJson, ordering, adb, mirror); default: runAll(), every phone-free spike
// --out      also write the results to this file
// --record   record the phone fixtures into fixtures/local/ instead (fixtures/README.md); --only picks which.
//            A banner in the app window says when to rotate the phone or move the screen, and says when a
//            take needs recording again (wrong orientation, a missing or unwanted rotation, too little or too
//            much change, or no usable video). The run then exits with code 2, or with 1 if a take failed
//            outright (and then doesn't write --out).
// --keep-serial  keep device serial numbers in the output; by default they become "(redacted)" wherever they
//            appear. The repository and home folders always become "<repo>" and "~". Results go into a
//            public repository.
//
// The window has to stay visible while it runs: WebView2 throttles timers in hidden windows.
import { execFileSync, execSync, spawn } from "node:child_process";
import { existsSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
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
const record = flag("--record");
const keepSerial = flag("--keep-serial");

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
/** Whether a session's size (width, height) is portrait. */
const portrait = ([width, height]) => width < height;
/**
 * What every take needs: video, and a frame in each orientation (describe-fixture.mjs rejects a session without
 * one). VideoStream sends a session's DecoderConfig with its first frame, so the counts match when each had one.
 */
const usable = ({ sessions, decoderConfigs }) =>
  sessions.length === 0
    ? "no video arrived"
    : decoderConfigs.length < sessions.length
      ? "an orientation ended before its first frame"
      : null;
/**
 * The phone fixtures (fixtures/README.md), recorded small enough to commit. The prompt shows before and during
 * each recording; `check` returns what's wrong with a usable take, so it can be redone while the phone is at hand.
 */
const RECORDINGS = [
  {
    name: "portrait-idle",
    seconds: 4,
    prompt: "Keep the phone still in portrait, with nothing moving on its screen.",
    check({ sessions, frames }) {
      if (sessions.length !== 1) return "the phone rotated";
      if (!portrait(sessions[0])) return "the phone was in landscape";
      // A still screen sends a frame, ten 100 ms repeats, then nothing (protocol/video.md §11.4).
      if (frames > 40) return `${frames} frames: the screen kept changing`;
      return null;
    },
  },
  {
    name: "rotation-midstream",
    seconds: 8,
    prompt: "Two seconds after recording starts, rotate the phone to landscape, wait a second, then rotate it back.",
    check({ sessions }) {
      if (sessions.length !== 3) return `${sessions.length} orientation(s) instead of three`;
      if (!portrait(sessions[0])) return "it started in landscape";
      return null;
    },
  },
  {
    name: "high-motion",
    seconds: 5,
    prompt: "Scroll the screen up and down fast, and keep going until this banner says it's recorded.",
    check({ sessions, frames }) {
      if (sessions.length !== 1) return "the phone rotated";
      if (!portrait(sessions[0])) return "the phone was in landscape";
      // Scrolling sends about 60 frames a second (the max_fps cap); a still screen, about a dozen in all.
      if (frames < 150) return `only ${frames} frames: the screen hardly moved`;
      return null;
    },
  },
];
const known = record ? RECORDINGS.map((r) => r.name) : Object.keys(CALLS);
for (const name of only ?? []) {
  if (!known.includes(name)) throw new Error(`unknown name ${name}; known: ${known.join(", ")}`);
}

if (flag("--build")) {
  execSync("npm run spikes:build", { cwd: join(repoRoot, "app"), stdio: "inherit" });
}
if (!existsSync(exe)) {
  console.error(redactor({}).scrub(`${exe} not found: pass --build first (it runs npm run spikes:build in app/)`));
  process.exit(1);
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
const PHONE_FREE = [
  "webcodecs", "replay", "replayRealtime", "throughputRaw", "throughputRawBurst", "throughputJson", "ordering", "adb",
];
const EXPECTED = only ?? (record ? RECORDINGS.map((r) => r.name) : PHONE_FREE);
/** Generous per-call limit: the longest spike (raw throughput) takes about 70 s. */
const CALL_TIMEOUT_MS = 10 * 60 * 1000;

/**
 * Results go into a public repository. A device serial can sit under a "serial" key, in the adb probe's
 * track-devices entries ("<serial> <state>") and errors, in Rust Debug output (`serial: "…"`) and in adb's
 * "device '…' not found", so every serial found is replaced wherever it appears, longest first, unless
 * --keep-serial. The repository and home folders can hold the Windows user name, so they become "<repo>"
 * and "~".
 */
function redactor(results) {
  const serials = new Set();
  JSON.stringify(results, (key, value) => {
    if (typeof value === "string") {
      if (key === "serial") serials.add(value);
      for (const m of value.matchAll(/serial: "([^"]+)"|device '([^']+)'/g)) serials.add(m[1] ?? m[2]);
    }
    return value;
  });
  for (const list of results.adb?.trackUpdates ?? []) {
    for (const entry of list) serials.add(String(entry).split(" ")[0]);
  }
  const known = keepSerial ? [] : [...serials].filter((s) => s.length >= 4).sort((a, b) => b.length - a.length);
  const escape = (part) => part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  // Either slash, any case: Windows paths are case-insensitive. The repository first, as it may be in the home.
  const folder = (dir) => new RegExp(dir.replace(/[\\/]+$/, "").split(/[\\/]/).map(escape).join("[\\\\/]"), "gi");
  const folders = [[folder(repoRoot), "<repo>"], ...(homedir().length > 3 ? [[folder(homedir()), "~"]] : [])];
  const scrub = (text) =>
    known.reduce(
      (t, serial) => t.replaceAll(serial, "(redacted)"),
      folders.reduce((t, [pattern, name]) => t.replace(pattern, name), text),
    );
  const replacer = (key, value) =>
    typeof value !== "string" ? value : !keepSerial && key === "serial" ? "(redacted)" : scrub(value);
  return { scrub, replacer };
}

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
  let appExited = false;
  app.on("exit", (code) => {
    appExited = true;
    failAll(`the app exited with code ${code}`);
  });
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
      // On a closed socket, send() drops the message silently, and the call would wait out its timeout.
      if (ws.readyState !== WebSocket.OPEN) return reject(new Error(`${method}: the DevTools connection is closed`));
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
      const { exception, text } = r.result.exceptionDetails;
      // Tauri's invoke() rejects with the command's Err(String): a primitive has a value, not a description.
      throw new Error(exception?.description ?? (exception && "value" in exception ? String(exception.value) : text));
    }
    return r.result?.result?.value;
  };
  await send("Runtime.enable");
  await send("Log.enable");
  /** What the page retains after a forced full GC, in MB: equal before and after a run means no leak (0C). */
  const retainedHeap = async () => {
    await send("HeapProfiler.collectGarbage");
    const { result } = await send("Runtime.getHeapUsage");
    const mb = (bytes) => (bytes === undefined ? null : Math.round((bytes / 1048576) * 10) / 10);
    return { usedMb: mb(result.usedSize), arrayBufferMb: mb(result.backingStorageSize) };
  };

  for (let i = 0; i < 60 && !(await evaluate("typeof window.__spikes === 'object'")); i++) await delay(500);
  if (!(await evaluate("typeof window.__spikes === 'object'"))) {
    throw new Error("window.__spikes never appeared: this isn't a spikes build; pass --build");
  }
  const results = { exe, power, build: await evaluate("navigator.userAgent") };
  const heapBefore = await retainedHeap().catch((e) => ({ error: String(e) }));
  if (record) {
    // The phone has to move during some recordings, so the window says what to do and when. Best-effort: a
    // failed banner mustn't end the run and lose the recordings already made.
    const banner = (text) =>
      evaluate(`{
        const el = document.getElementById("record-banner") ?? document.body.appendChild(document.createElement("div"));
        el.id = "record-banner";
        el.style.cssText = "position:fixed;top:0;left:0;right:0;z-index:1;padding:16px;font:600 20px system-ui;background:#000;color:#fff";
        el.textContent = ${JSON.stringify(text)};
      }`).catch(() => {});
    await banner(
      "These recordings go into a public repository. Show a screen with nothing personal on it: no photos, faces, " +
        "account pictures, names, notifications or documents. Turn on Do Not Disturb.",
    );
    await delay(8000);
    for (const { name, seconds, prompt, check } of RECORDINGS.filter((r) => !only || only.includes(r.name))) {
      if (appExited) break;
      for (let s = 5; s > 0; s--) {
        await banner(`Get ready for ${name} (${seconds} s): ${prompt} Recording starts in ${s}…`);
        await delay(1000);
      }
      await banner(`Recording ${name}: ${prompt}`);
      process.stderr.write(`recording ${name}…\n`);
      const options = { seconds, maxSize: 480, videoBitRate: 1_000_000, recordAs: name };
      results[name] = (await evaluate(`window.__spikes.mirror(${JSON.stringify(options)})`).catch((e) => ({
        error: String(e),
      }))) ?? { error: "the page returned no result" };
      const stream = results[name].rust?.stream;
      if (!stream && !results[name].error) results[name].error = "no stream came back";
      const problem = results[name].error ? `it failed (${results[name].error})` : (usable(stream) ?? check(stream));
      if (problem) {
        results[name].takeProblem = problem;
        process.stderr.write(redactor(results).scrub(`${name} needs recording again: ${problem}\n`));
      }
      const verdict = problem ? `${name} needs recording again: ${problem}.` : `${name} recorded.`;
      await banner(redactor(results).scrub(verdict));
      await delay(1500);
    }
    const redo = RECORDINGS.filter((r) => results[r.name]?.takeProblem).map((r) => r.name);
    await banner(
      redo.length
        ? `Done. Record again: node scripts/run-spikes.mjs --record --only ${redo.join(",")}`
        : "Done recording. You can put the phone down.",
    );
    await delay(3000);
  } else if (only) {
    for (const name of only) {
      process.stderr.write(`running ${name}…\n`);
      results[name] = await evaluate(CALLS[name]).catch((e) => ({ error: String(e) }));
    }
  } else {
    process.stderr.write("running every phone-free spike (about 90 s)…\n");
    Object.assign(results, await evaluate("window.__spikes.runAll()"));
  }
  results.retainedHeap = { before: heapBefore, after: await retainedHeap().catch((e) => ({ error: String(e) })) };
  results.pageErrors = problems;
  ws.close();
  stop();
  const adbPidsAfter = adbPids();
  results.adbServer = {
    pidsBefore: adbPidsBefore,
    pidsAfter: adbPidsAfter,
    untouched: adbPidsBefore.length > 0 && adbPidsBefore.join() === adbPidsAfter.join(),
  };

  // Never overwrite a results file with an incomplete run. A take that needs recording again is still a
  // measurement, so it's written, but the exit code says so.
  const failed = EXPECTED.filter((name) => !results[name] || results[name].error);
  const redo = record ? EXPECTED.filter((name) => results[name]?.takeProblem) : [];
  const json = JSON.stringify(results, redactor(results).replacer, 2);
  if (failed.length) {
    console.error(`incomplete run, missing or failed: ${failed.join(", ")}${out ? `; not writing ${out}` : ""}`);
    exitCode = 1;
  } else {
    if (out) writeFileSync(out, json + "\n");
    if (redo.length) {
      console.error(`record again: node scripts/run-spikes.mjs --record --only ${redo.join(",")}`);
      exitCode = 2;
    }
  }
  console.log(json);
} catch (e) {
  console.error(redactor({}).scrub(String(e)));
  exitCode = 1;
} finally {
  stop();
}
process.exit(exitCode);
