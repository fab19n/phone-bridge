# PhoneBridge

Mirror and control an Android phone from a Windows PC over USB.
Tauri v2 desktop app (Rust backend, React + WebCodecs frontend) driving the unmodified
[scrcpy](https://github.com/Genymobile/scrcpy) server over ADB.

> Status: **Stage 0 (foundations) done locally**; the first GitHub CI run is pending. Next: Stage 1 validation spikes.
> See [docs/DEVELOPMENT_PLAN.md](docs/DEVELOPMENT_PLAN.md).

## Repository layout

| Path | What |
|---|---|
| `app/` | The Tauri app: `src/` (React), `src-tauri/` (Rust) |
| `protocol/` | Wire formats PhoneBridge implements: `video.md`, `control.md` |
| `spikes/phase0/` | Stage 1 validation harness (from the original scaffold, not yet buildable) |
| `fixtures/` | Recorded scrcpy streams for deterministic tests |
| `companion/` | Android companion app (parked until Stage 8) |
| `reference/scaffold/` | Original React scaffold, kept for reference; superseded by `app/` in Stages 3–4 |
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
custom protocol instead: `npm run tauri build -- --debug --no-bundle`, then run `src-tauri/target/debug/phonebridge.exe`
(right-click → Inspect opens DevTools). The next `tauri dev` turns that binary back into a dev build.

Before pushing, run the checks CI runs:

```bash
cd app
npm run typecheck && npm run lint && npm test
cd src-tauri
cargo fmt --all -- --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked --all-targets
```

CI also builds the NSIS installer (`npm run tauri build -- --bundles nsis`).

To download the pinned scrcpy-server into `app/src-tauri/binaries/` (hash-checked):

```bash
powershell -ExecutionPolicy Bypass -File scripts/fetch-scrcpy-server.ps1
```
