# PhoneBridge — Development Stages Plan

**Created:** 2026-11-01 · **Based on:** `docs/REVIEW.md` (finding IDs like B1 and A2 refer to that file)
**Platform:** Windows 10/11 desktop (Tauri v2 + React + Rust), Android via ADB + scrcpy-server
**Key change from the spec:** the MVP ships **without the companion app** (REVIEW A2). The companion becomes Stage 8, built only once it unlocks something ADB can't.

Every stage ends with an **exit gate**. Don't start the next stage until the gate passes. Durations assume one full-time developer.

| Stage | Name | Duration | Milestone |
|---|---|---|---|
| 0 | Foundations & product definition | 3–4 days | Repo builds in CI; PRD drafted (sign-off gates Stage 2) |
| 1 | Validation spikes (revised Phase 0) | 1 week | Architecture go / no-go |
| 2 | Rust streaming core | 2 weeks | Packets flowing, replay fixtures |
| 3 | Decode & render | 1.5 weeks | Live screen in the app |
| 4 | Input & control | 1.5 weeks | **Tech demo:** mirror + control |
| 5 | Device management & state machine | 1.5 weeks | Every error has a UI path |
| 6 | Packaging & Windows hardening | 1 week | **Alpha:** installable mirror + control |
| 7 | Product shell & quick wins | 2 weeks | **Beta = the MVP** (PRD §5): settings, tray, clipboard, drag-drop |
| 8 | Companion app (decision gate) | 3+ weeks | Only if PRD needs phone-side features |
| — | **Total to Beta** | **~11–12 weeks** | |

---

## Stage 0 — Foundations & product definition (3–4 days)

**Product**
- [~] Write a one-page PRD covering target user, the wedge feature, business model, and "casting vs mirroring" (REVIEW A1, Part D). → Draft in `docs/PRD.md` with a competitor table; owner answered Q3–Q6 on 2026-11-01 (free; mirroring + control; Windows now, macOS later; no companion in MVP). **Still open:** confirm the segment sequencing (all three segments, B first), pick the wedge, and choose open- vs closed-source.
- [~] Set numeric targets: latency ≤ 70 ms p50 / ≤ 120 ms p95 (USB, 1080p60); install-to-first-mirror < 3 min; crash-free sessions > 99%.

**Engineering**
- [x] `git init`, `.gitignore`, a branch strategy, and this repo layout:
  ```
  bridge/
    app/            # Tauri app (src/ React, src-tauri/ Rust)
    companion/      # Android (Stage 8; keep scaffold code parked here)
    protocol/       # video.md, control.md, (later) pairing.md
    spikes/phase0/  # Stage 1 harness (Stage 1 put the real harness in the app instead; this moved to reference/scaffold/phase0-tests)
    fixtures/       # recorded scrcpy streams
    docs/
  ```
- [x] Extract `phonebridge-scaffold.tar.gz` and **delete the junk `{phase0-tests…}` brace directories** (REVIEW B15).
- [x] Generate the real project with `npm create tauri-app@latest` (react-ts), then move the scaffold files in. → `app/`; the React scaffold is parked in `reference/scaffold/` (superseded in Stages 3–4).
- [x] Fix `Cargo.toml`: add `rand`. Remove `serde_bytes` if you go raw-binary (B1).
- [x] **Pin scrcpy:** download the release, record the JAR's exact version string, store it in one place, and save the JAR's SHA-256 (B15). → **v5.0.1**: `SERVER_VERSION` and `SERVER_SHA256` in `crates/scrcpy/src/lib.rs` (moved there from `app/src-tauri/src/scrcpy.rs` in Stage 1) are the single source of truth; `scripts/fetch-scrcpy-server.ps1` reads both from that file and verifies the download.
- [x] Set up CI (GitHub Actions, `windows-latest`): `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `tsc --noEmit`, `eslint`, `vitest`, and a `tauri build` smoke test. → `.github/workflows/ci.yml` + Dependabot. Every CI step passes on a clean export of the tree (checked locally 2026-11-02), and `Cargo.lock` is committed. The first GitHub run, on the Stage 0 push, passed all three jobs, including the NSIS build ([run 41336726552](https://github.com/fab19n/phone-bridge/actions/runs/41336726552); its commit has the same tree as `d813156`). The Stage 1 workspace layout has passed all three on GitHub too ([run 41462331588](https://github.com/fab19n/phone-bridge/actions/runs/41462331588), on `2a30541`).
- [x] Write `protocol/video.md` and `protocol/control.md` from the scrcpy source for the pinned version.
- [x] Check licensing: scrcpy Apache-2.0 NOTICE; ADB redistribution terms. → `docs/LICENSING.md` (open questions for a lawyer included).

**Exit gate:** a clean clone builds in CI and the scrcpy version is pinned. The PRD's remaining decisions (segment sequencing, wedge, open vs closed source) gate **Stage 2**, not Stage 1: the Stage 1 spikes don't depend on them.

> **Status 2026-11-02:** engineering done and committed. Verified locally: the frontend checks and build, `cargo fmt`/`clippy -D warnings`/`test`, and the app running in dev and release builds (the release build enforces the CSP, and IPC goes over `http://ipc.localhost`). CI passed on GitHub too (see above), so the exit gate is met. Still open: the PRD decisions, which gate Stage 2. Legend: `[x]` done, `[~]` started but not finished (in Stage 0: drafted and awaiting a decision; in Stages 2 and 4: built and tested in Stage 1).

---

## Stage 1 — Validation spikes: revised Phase 0 (1 week)

Same goals as the spec's Phase 0, but each test measures the right thing, plus four new tests.

| # | Test | Pass criteria | If it fails |
|---|---|---|---|
| 0A | scrcpy server streams over ADB | Framed stream parsed; `connect → dummy byte` **retry loop** works on a cold start (B9) | Check version string, USB debugging |
| 0B | WebCodecs in WebView2 | Decodes real frames from a **recorded fixture**, codec string **parsed from SPS** (B3) | Decode in Rust (FFmpeg) → re-architect |
| 0C | Raw-byte Channel throughput | `InvokeResponseBody::Raw`; **Rust-recv → JS-onmessage** p95 < 5 ms at 60 × 50 KB/s for 60 s; flat JS heap (B1, B2) | Local WebSocket on 127.0.0.1 with a token |
| 0D | End-to-end | Phone screen live in canvas; config prepended to keyframe; **rotation** survives (B3, B14) | Debug stage by stage |
| **0E** | Glass-to-glass latency | Film phone + PC with a 240 fps camera (or an on-screen timer); record p50/p95 against the Stage 0 target | Tune bitrate, `max_size`, render path |
| **0F** | Control ordering | 1,000 rapid `DOWN/MOVE/UP` events through a single-writer queue arrive in order (sequence numbers) (B6) | Serial sender, one invoke in flight (chosen: see the status below) |
| **0G** | ADB coexistence | With Android Studio's adb server running, PhoneBridge works **without restarting it**, using the direct 5037 protocol (B10) | Choose option (b) or (c) from B10 |
| **0H** | Packaged sidecar | NSIS build installs (Tauri's default is per-user, `%LOCALAPPDATA%\PhoneBridge`; also test per-machine if Stage 6 picks it); `adb.exe` starts with its DLLs; no console windows (B4, B9) | Ship adb + DLLs as resources, spawn from Rust |

- [x] **Record 3 fixture streams** (portrait, rotation mid-stream, high-motion) into `fixtures/`. Every later stage tests against these. Keep them small enough to commit (`fixtures/README.md`). → `fixtures/portrait-idle.bin`, `rotation-midstream.bin` and `high-motion.bin`, from a Galaxy A56 on Android 17 (0.76 MB in all), replayed by `crates/scrcpy/tests/phone_fixtures.rs` against a separate JavaScript parse. `fixtures/synthetic-rotation.bin` (`scripts/make-synthetic-fixture.mjs`) stays as a second, synthetic source.
- [x] Write a short "Phase 0 results" note in `docs/` with numbers. → `docs/STAGE1_RESULTS.md`, raw data in `docs/stage1-results.json`, `stage1-results-earlier.json`, `stage1-results-ac.json` and `stage1-results-phone.json`.

**Exit gate:** all of 0A–0H pass (or have a chosen fallback), and the measured latency is within the target.

> **Status 2026-11-02:** on a Galaxy A56 (Android 17), 0A, 0B and 0D pass, including a live rotation, and 0G passes with the phone attached. 0F passes with the serial sender. 0C passes on AC: every frame delivered in order with a flat retained heap, and p95 3.0–3.2 ms in four of five runs (the fifth was slower throughout, at 5.3 ms, with one 333 ms stall). On battery it measured 6.7–6.8 ms in three runs, over the 5 ms target but under a tenth of the 70 ms budget. The three phone fixtures are recorded. Still open: 0E (glass-to-glass latency), 0G with Android Studio open, and 0H, deferred until the adb distribution option is chosen (`docs/LICENSING.md` §3.5). Numbers: `docs/STAGE1_RESULTS.md`. The harness lives in the app behind the `spikes` cargo feature (`npm run tauri:spikes`, or `scripts/run-spikes.mjs` unattended), not in `spikes/phase0`.

---

## Stage 2 — Rust streaming core (2 weeks)

Build the `src-tauri` modules from spec §1.3, with these corrections. The protocol code lives in the Tauri-free crates `crates/adb` and `crates/scrcpy`, which Stage 1 started; `[~]` means built and tested in Stage 1, but not finished yet.
- [~] `adb/`: an ADB client that talks to the server over **TCP 5037**: host services `host:devices-l`, `host:track-devices`, `host:transport:<serial>` and `host-serial:<serial>:features` (plus `forward:norebind:`, `killforward:` and `host:list-forward` for the forward fallback); device services `sync:` (push), `shell,v2,raw:` (legacy `shell:` fallback) and `localabstract:<name>`. PhoneBridge never runs the `adb` CLI per session. It spawns an adb binary only to *start* a server when none is running (B10; which binary depends on `docs/LICENSING.md` §3). All spawns use `CREATE_NO_WINDOW`. → `crates/adb` (phonebridge-adb). It also addresses devices by transport id and reads the server address the way AOSP's client does (`ADB_SERVER_SOCKET` first). Tested against a fake server, and against the local adb server in the Stage 1 spikes: in all twelve phone sessions (Galaxy A56, Android 17) it pushed the jar over `sync:`, started the server over `shell,v2,raw:` and opened the sockets over `localabstract:`. The forward and legacy `shell:` fallbacks and transport-id addressing have run only against the fake server, and starting a server has been tried only with a missing adb binary.
- [~] `scrcpy/server.rs`: push the JAR (check its hash first), launch the server over a `shell,v2,raw:` stream the session owns (drain its stdout/stderr into the logs), and open the sockets with a random `scid`: preferably straight to `localabstract:scrcpy_<scid>` over `host:transport` (no TCP port, nothing to clean up), else a `forward:norebind:tcp:0` fallback removed right after connecting. Close everything on drop (B9, `protocol/video.md` §13). → The command line is `phonebridge_scrcpy::server`. The connect sequence is spike code in `app/src-tauri/src/spikes/mirror.rs`, run on a Galaxy A56 (Android 17) in Stage 1; Stage 2 turns it into the session actor.
- [~] `scrcpy/video.rs`: connect with retry; parse dummy byte, 64-byte device meta, **4-byte codec id**, then 12-byte headers that are either a **session packet** (bit 63 set: flags, width, height; sent at start and on every rotation/resize) or a media header (bit 62 = config, bit 61 = key frame, 61-bit µs PTS). This is the v4.0+ layout; the spec and Phase 0 harness use the old v3 layout (REVIEW B16, `protocol/video.md` §7–9). Test it with **unit tests against `fixtures/`**. Merge each H.264/H.265 config packet into the next media packet and parse its SPS into the `avc1.PPCCLL` codec string (`protocol/video.md` §10, §13), so config-only packets never reach the frontend. Fixture tests: every key frame sent starts with SPS/PPS, and the codec string is right. → `crates/scrcpy` (video, h264, control). Use `video::PacketReader` in loops that can time out: it is cancel safe, `read_packet` isn't.
- [ ] `session/`: a `MirrorSession` actor (tokio task) that owns the sockets, the server child and both Channels. `start_mirror` and `stop_mirror` are real commands. Stopping is idempotent.
- [x] Video Channel: raw bytes plus a 17-byte header (`flags u8 | pts u64 | recv_us u64`; flags bit 0 = key frame, bit 1 = decoder-config record, whose payload is the codec string plus the session packet's width and height). Decoder-config records travel **in-band**, just before the key frame they configure: Tauri orders messages only within one Channel, so a config sent on another channel could overtake or trail its frames. The JSON Events Channel carries lifecycle only (started, stopped, errors). → `app/src-tauri/src/video_channel.rs` and `app/src/lib/videoChannel.ts`, which test against the same golden bytes.
- [ ] Logging: `tracing` with a rolling file in the app log dir (needed for support later).

**Exit gate:** `cargo test` parses all fixtures. A real device streams for 30 minutes with no leaked processes or forwards (check `adb forward --list` and Task Manager), the page's heap after a forced GC ends no bigger than it started, and every Rust-to-page delivery over 50 ms is logged (0C saw a 333 ms stall; on a phone, gaps of 100 ms or more between frames are normal, see the quiet-stream risk).

---

## Stage 3 — Decode & render (1.5 weeks)

- [ ] `lib/decoder.ts` rewrite: decode the merged access units from Rust. It never sees a config-only chunk, and SPS parsing lives in Rust (Stage 2). On each decoder-config record, call `configure()` with its codec string and `codedWidth`/`codedHeight`, then skip delta chunks until the next key chunk. That happens once per session packet (rotation, `RESET_VIDEO`, `downsize_on_error`). Use `optimizeForLatency: true`.
- [ ] Back-pressure (B5): render only the newest frame (close the rest). When the queue is too deep, drop **until the next keyframe** and send `RESET_VIDEO` to get one quickly.
- [ ] Rendering: `drawImage` on a 2D canvas to start. Try WebGL or `transferControlToOffscreen` + a worker only if the Stage 1 numbers say you need to.
- [ ] `LatencyOverlay.tsx`: recv → onmessage → decode-out → paint, as p50/p95.
- [ ] Fix the stale-closure bugs (B8). Mirror state comes from Rust or a reducer, not from captured `useState` values.

**Exit gate:** all fixtures decode with zero errors in a vitest/browser test, rotation works live, and the overlay shows latency within the target.

---

## Stage 4 — Input & control (1.5 weeks) → Tech demo

- [~] `scrcpy/control.rs`: serialise the control messages for the pinned version (touch, scroll, keycode, text, back-or-screen-on, set/get clipboard, reset video, rotate). Golden-byte unit tests against `protocol/control.md`. → Built in Stage 1 as `phonebridge_scrcpy::control`, with golden-byte tests from `protocol/control.md`. Nothing has been sent to a device yet.
- [ ] Ordered input (B6): the page sends control events through a **serial sender**, with one invoke in flight and the events that queue up meanwhile sent as one batch. Stage 1 test 0F showed that fire-and-forget invokes arrive out of order, even for sync commands. In Rust, one **single writer task** behind an `mpsc` queue owns the control socket.
- [ ] `lib/coordinates.ts` rewrite: map to the **current frame size**, subtract the letterbox, include `devicePixelRatio`, and send the frame size it mapped against with each touch. Rust forwards that size unchanged, so the server drops touches computed for a stale frame (B7, `protocol/control.md` B7).
- [ ] `hooks/useControl.ts`: pointer capture; multi-touch is out of scope; a native non-passive wheel listener; keyboard → keycodes with meta-state; printable text → `INJECT_TEXT` in batches; **Ctrl+V → set clipboard + paste**.
- [ ] `ControlBar.tsx`: Back / Home / Recents / Vol± / Power / Rotate / Screenshot.

**Exit gate (Tech demo):** tap, swipe, scroll, type and navigate work on 3 devices, and taps land within 2 px of the cursor at any window size or rotation.

---

## Stage 5 — Device management & state machine (1.5 weeks)

- [ ] A `SessionManager` in Rust as the **single source of truth** (B12). It runs the `ConnectionState` enum from the spec §5.5, minus the pairing states, plus `Unauthorized` and `Offline`. On every transition it emits one `state-changed` snapshot.
- [ ] Device discovery via **`host:track-devices`** (push, not polling).
- [ ] Implement every row of the spec's §5.6 error matrix (minus the companion rows) with a message and a recovery action. Add one test per row using a fake ADB server. For "No device", don't assume the cable is out: suggest re-plugging or toggling USB debugging, and don't restart the adb server (B10). In Stage 1 (0G), a re-plugged phone wasn't listed for minutes although Windows showed its ADB interface as working, then was listed again without a restart.
- [ ] Dashboard: device cards from the snapshot; a **USB-debugging onboarding wizard** with per-manufacturer steps and live "unauthorized → authorized" detection (A3).
- [ ] Auto-reconnect when a cable is replugged during a session.

**Exit gate:** unplug, deny the prompt, kill adb or kill the server mid-session, and each one shows the right message and recovers without restarting the app.

---

## Stage 6 — Packaging & Windows hardening (1 week) → Alpha

- [ ] NSIS installer with the corrected sidecar/DLL layout (B4); install, upgrade and uninstall cycle; shortcuts.
- [ ] Choose the shipping install mode: per-user (Tauri's default, no admin prompt) or per-machine, via `bundle.windows.nsis.installMode`.
- [ ] **Tighten capabilities:** no `shell:*` in the WebView, scoped `fs`, a strict CSP (B13).
- [ ] Clean shutdown kills every child and removes forwards, including on crash (a Job Object so children die with the app).
- [ ] Code signing: self-signed for Alpha; budget an OV/EV certificate before public release.
- [ ] Test matrix: Windows 10 + 11 × at least 5 Android devices (Samsung, Pixel, Xiaomi, OnePlus, one Android 10 device).

**Exit gate (Alpha):** a non-developer tester goes from download to mirroring in < 3 minutes, using only the in-app guide.

---

## Stage 7 — Product shell & quick wins (2 weeks) → Beta (the MVP)

- [ ] Settings (spec §5.2) via `tauri-plugin-store`, with a `schemaVersion` field from day one. Settings feed the scrcpy launch arguments.
- [ ] System tray; minimize-to-tray that **respects the setting**; start minimized; window state persistence.
- [ ] Quick wins (A4): **two-way clipboard sync**, **drag-and-drop APK install / file push**, **turn screen off while mirroring**, stay-awake.
- [ ] Screenshot (save the current decoded frame to PNG; no device round-trip needed).
- [ ] Optional: audio forwarding (Android 11+) if the PRD calls for it.
- [ ] Crash reporting and opt-in telemetry (only if the PRD/business model wants it).

**Exit gate (Beta):** the PRD's success metrics are met on the test matrix, there are no P1 bugs open, and user setup docs are written.

---

## Stage 8 — Companion app (decision gate, 3+ weeks)

Start this only if the PRD's wedge needs phone-side features ADB can't provide: notifications, SMS/calls, background clipboard, or Wi-Fi discovery.
- [ ] Design `protocol/pairing.md` properly: Ed25519 identity keys, a short code shown on both screens, Noise-XX (or similar) authenticated channel, protection against socket squatting (B11).
- [ ] Fix the Android 12–14 foreground-service rules (prerequisite permissions for `connectedDevice`; no background FGS starts). Replace the USB broadcast receiver with an approach that actually fires.
- [ ] Replace `security-crypto` with DataStore + a Keystore-wrapped key.
- [ ] Then the spec's Phase 3+ (files, Wi-Fi via ADB-TCP, etc.).

---

## Risk register additions (on top of spec §7)

| Risk | Likelihood | Impact | Mitigation | Stage |
|---|---|---|---|---|
| Bundled adb restarts the user's adb server | High | High | Direct 5037 protocol (B10) | 1, 2 |
| Video sent as JSON wastes CPU (0C measured +1.3–3.4 ms p95 in four of five runs, and 4.7–9.5× the Rust send cost) | Avoided | Low | Raw-byte channel (B1) | 1 |
| Channel latency on battery misses the 0C target (p95 6.7–6.8 ms on battery vs 3.0–3.2 ms in four of five AC runs) | Certain on battery | Low (under 10% of the 70 ms budget) | Keep < 5 ms as the AC criterion; judge battery by 0E's glass-to-glass result, measured on both power states | 1 |
| Occasional Channel stall (one 333 ms gap in nine 60-second runs; cause unknown) | Low | Medium (a visible freeze) | The runner logs gaps over 50 ms; Stage 2's 30-minute stream logs every delivery over 50 ms (on a phone, gaps between frames alone are normal) | 2 |
| A still screen goes silent: about a second of repeated frames, then nothing until it changes | Certain | Medium (a frame-silence watchdog would end healthy sessions) | Judge session health by the sockets and the server's shell stream, not by frame arrival | 2 |
| Control events reorder between the page and Rust | Certain with fire-and-forget invokes (0F) | High | Serial sender, one invoke in flight (B6) | 4 |
| DLL layout works in dev but breaks when installed | High | High | Test 0H in Stage 1 (B4) | 1 |
| Product looks like a free scrcpy GUI | High | Critical | PRD wedge before Stage 2 (A1) | 0 |
| Encoder profile/level ≠ hard-coded codec string | Medium | High | Parse SPS (B3) | 3 |
| WebView XSS gives `adb shell` access | Low | Critical | Remove shell capabilities (B13) | 6 |
