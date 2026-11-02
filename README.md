# PhoneBridge

Mirror and control an Android phone from a Windows PC over USB.
Tauri v2 desktop app (Rust backend, React + WebCodecs frontend) driving the unmodified
[scrcpy](https://github.com/Genymobile/scrcpy) server over ADB.

> Status: **Stage 1 (validation spikes)**: the phone-free spikes pass on AC power. On battery, 0C runs over its target
> (p95 6.7–6.8 ms vs < 5 ms; 3.0–3.2 ms on AC). 0A, live 0D, 0E and 0G with a device need a phone; 0H waits on the
> adb distribution decision.
> See [docs/STAGE1_RESULTS.md](docs/STAGE1_RESULTS.md) and [docs/DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md).

## Repository layout

| Path | What |
|---|---|
| `app/` | The Tauri app: `src/` (React), `src-tauri/` (Rust) |
| `crates/` | Tauri-free Rust crates: `adb` (adb server protocol), `scrcpy` (scrcpy v5.0.1 wire protocol) |
| `protocol/` | Wire formats PhoneBridge implements: `video.md`, `control.md` |
| `fixtures/` | Recorded scrcpy streams for deterministic tests |
| `companion/` | Android companion app (parked until Stage 8) |
| `reference/scaffold/` | The original scaffold (React app, Phase 0 harness), kept for reference only |
| `docs/` | Spec review, development plan, PRD, licensing |
| `scripts/` | Developer scripts (e.g. fetching the pinned scrcpy-server) |

## Prerequisites (Windows)

- Node.js 22.13+ or 24 (CI uses 24)
- Rust stable via [rustup](https://rustup.rs) with the MSVC toolchain
- Visual Studio Build Tools with the **Desktop development with C++** workload (Rust needs its linker), plus the
  other [Tauri prerequisites](https://tauri.app/start/prerequisites/) (WebView2 ships with Windows 10/11)
- Android platform-tools (`adb`) for device testing

## Develop

```bash
cd app
npm install
npm run tauri dev
```

`tauri dev` loads the Vite dev server directly, so **no CSP is applied**. To check CSP or IPC changes, build with the
custom protocol instead: `npm run tauri build -- --debug --no-bundle`, then run `target/debug/phonebridge.exe` (repo root)
(right-click → Inspect opens DevTools). The next `tauri dev` turns that binary back into a dev build.

Before pushing, run the checks CI runs:

```bash
cd app
npm run typecheck && npm run lint && npm test
cd ..
cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked --workspace --all-targets
cargo clippy --locked -p phonebridge --all-targets --features spikes -- -D warnings && cargo test --locked -p phonebridge --features spikes
```

CI also builds the NSIS installer (`npm run tauri build -- --bundles nsis`).

### Stage 1 spikes

`npm run tauri:spikes` (in `app/`) opens the spikes page: a build with the `spikes` cargo feature that normal builds
never include. To run every phone-free spike unattended and save the numbers (keep the window visible while it runs,
since WebView2 throttles hidden windows):

```bash
node scripts/run-spikes.mjs --build --out docs/stage1-results.json
```

`scripts/make-synthetic-fixture.mjs` regenerates the replay fixture (it needs ffmpeg with libx264).

To download the pinned scrcpy-server into `app/src-tauri/binaries/` (hash-checked):

```bash
powershell -ExecutionPolicy Bypass -File scripts/fetch-scrcpy-server.ps1
```
