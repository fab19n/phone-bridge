# PhoneBridge — Stage 1 Results (Validation Spikes)

**Date:** 2026-11-02 · **Plan:** `docs/DEVELOPMENT_PLAN.md`, Stage 1
**Raw data:**
- `docs/stage1-results.json`: the last run on battery, `final` below (the runner records the power state itself).
- `docs/stage1-results-earlier.json`: the earlier runs the same day. `firstRun`: power not recorded. `orderingRun`: 0F only. `batteryRun`: a full run. `batteryRepeat`: 0C only.
- `docs/stage1-results-ac.json`: five later runs on AC power. `acRun` and `acRun2`: full runs. `acRepeat1`, `acRepeat2` and `acHeapCheck`: 0C only. From `acRepeat1` on, the runner logs arrival gaps over 50 ms; `acHeapCheck` adds the retained-heap check.

**Machine:** Intel Core Ultra 7 258V (8 cores), 32 GB RAM, Intel Arc 140V, Windows 11 Pro build 26200, WebView2 156.0.4314.58. **Build:** release, with the `spikes` feature.
**No phone was connected.** Everything below runs on the synthetic fixture or on loopback.
**Reproduce:** `node scripts/run-spikes.mjs --build --out docs/stage1-results.json`. Keep the window visible while it runs. The runner records AC vs battery, the adb server's process ids and the page's heap after a forced GC, and it won't write an incomplete run.

## Summary

| # | Test | Status | Result |
|---|---|---|---|
| 0A | scrcpy server streams over ADB | ⏳ Needs a phone | Implemented (`spike_mirror`): push over `sync:`, the server over `shell,v2,raw:`, and the video socket over `localabstract:`. It retries only while the server isn't listening yet. Not run yet |
| 0B | WebCodecs in WebView2 | ✅ Pass on the synthetic fixture · ⏳ recorded fixture | H.264 Baseline, Main and High (levels 3.1–5.1) are supported, with and without a hardware preference. All 120 fixture frames decode with 0 errors, under 1 ms per frame (p50) |
| 0C | Raw-byte Channel throughput | ✅ Pass on AC · ⚠ over target on battery | 60 × 50 KB/s for 60 s, every frame delivered in order in all nine runs. On AC, p95 3.0–3.2 ms in four of five runs; the fifth was slower throughout (5.3 ms) and had one 333 ms stall. On battery, 6.7–6.8 ms in three runs (target < 5 ms). The heap is flat: 1.2 MB retained before a run, 1.4 MB after |
| 0D | End-to-end | ✅ Pass on the fixture · ⏳ live | Across the rotation the decoder was reconfigured, frames went from 480x1040 to 1040x480, the canvas followed, and there were 0 errors |
| 0E | Glass-to-glass latency | ⏳ Needs a phone and a camera | PC side measured so far: 2.3–3.9 ms delivery on AC (4.3–6.5 ms on battery) and at most 1.2 ms decode, before the next display refresh |
| 0F | Control ordering | ✅ Pass with a design change | Fire-and-forget invokes reorder, even sync commands. A serial sender (one invoke in flight) gives 0 inversions |
| 0G | ADB coexistence | ✅ Pass with no device · ⏳ with one | The 5037 client used the running adb server (protocol 41) without restarting it: the same process (PID 42004) before the app started and after it was killed |
| 0H | Packaged sidecar | ⏸ Deferred | Only matters if the installer ships an `adb.exe` (options 1 or 4 in `docs/LICENSING.md` §3.5). That needs a decision first |

**Exit gate: not met yet.** Every spike that can run without hardware passes on AC. On battery, 0C runs over its target, but at under a tenth of the 70 ms latency budget, so it changes no design decision (see 0C). Still needed:
- A phone, for 0A, live 0D, 0E, 0G with a device, and the three recorded fixtures (which also complete 0B).
- The adb distribution decision, for 0H.

## 0B — WebCodecs in WebView2

- **Support matrix.** `VideoDecoder.isConfigSupported` at 1080x1920 with `optimizeForLatency: true` reports 14 of 16 configurations supported, the same in every run.
  - Supported, with both `prefer-hardware` and `no-preference`: `avc1.42E01F` (Baseline 3.1), `avc1.4D401F` (Main 3.1), `avc1.640028`, `avc1.640029`, `avc1.64002A` and `avc1.640033` (High 4.0 to 5.1), and `av01.0.08M.08` (AV1).
  - Not supported either way: HEVC (`hvc1.1.6.L120.90`). PhoneBridge uses H.264, so this doesn't matter. AV1 is an option for later.
- **Real decode, fixture `synthetic-rotation`.** The fixture is real H.264 from libx264 (High 4.0, 480x1040, then 1040x480), framed exactly as scrcpy v5.0.1 sends it.
  - 120 of 120 frames decoded, 0 skipped, 0 errors.
  - Rust derives the codec string `avc1.640028` from the SPS and merges the config packet into the key frame (REVIEW B3).
  - Paced at 30 fps, decoding one frame takes p50 0.7–0.9 ms and p95 1.2 ms on battery, and p50 0.4–0.5 ms and p95 0.5–0.7 ms on AC.
- **Not covered yet:** a real phone's encoder output (its profile, level and cropping), which is what the plan's criterion asks for. The recorded fixtures will cover it.

## 0C — Raw-byte Channel throughput

Rust sends synthetic frames at a fixed rate from its own thread. The page measures each message's delivery time against `recv_us`; both sides read the system clock.

| Run | Power | Payload | Frames | Latency p50 / p95 / p99 / max (ms) | Rust send p50 | Heap MB (start / max / end) |
|---|---|---|---|---|---|---|
| firstRun | not recorded | 50 KB raw, 60 s | 3600/3600 | 3.1 / 3.9 / 4.7 / 11.3 | 76 µs | 1.9 / 47.9 / 13.4 |
| batteryRun | battery | 50 KB raw, 60 s | 3600/3600 | 5.8 / 6.7 / 7.6 / 19.5 | 72 µs | 4.3 / 47.2 / 11.7 |
| batteryRepeat | battery | 50 KB raw, 60 s | 3600/3600 | 5.8 / 6.8 / 7.9 / 22.7 | 70 µs | 1.9 / 48 / 13.5 |
| final | battery | 50 KB raw, 60 s | 3600/3600 | 5.8 / 6.8 / 7.6 / 15.8 | 72 µs | 4.3 / 47.2 / 11.8 |
| acRun | AC | 50 KB raw, 60 s | 3600/3600 | 3.1 / 5.3 / 64.2 / 319.5 | 47 µs | 4.3 / 44 / 11.5 |
| acRepeat1 | AC | 50 KB raw, 60 s | 3600/3600 | 2.5 / 3.0 / 3.3 / 6.3 | 35 µs | 1.9 / 47.9 / 13.7 |
| acRepeat2 | AC | 50 KB raw, 60 s | 3600/3600 | 2.5 / 3.1 / 3.8 / 9.4 | 38 µs | 1.9 / 47.9 / 13.5 |
| acRun2 | AC | 50 KB raw, 60 s | 3600/3600 | 2.6 / 3.2 / 4.4 / 7.5 | 41 µs | 4.2 / 47.1 / 11.6 |
| acHeapCheck | AC | 50 KB raw, 60 s | 3600/3600 | 2.7 / 3.2 / 4.2 / 10.9 | 35 µs | 1.4 / 12.4 / 11.3 |
| firstRun | not recorded | 200 KB raw, 10 s | 600/600 | 4.7 / 5.4 / 6.0 / 12.2 | 102 µs | 13.4 / 30.8 / 7.6 |
| batteryRun | battery | 200 KB raw, 10 s | 600/600 | 7.3 / 8.7 / 9.6 / 13.1 | 123 µs | 11.7 / 30.7 / 8.8 |
| final | battery | 200 KB raw, 10 s | 600/600 | 8.0 / 9.4 / 10.0 / 10.6 | 152 µs | 11.8 / 33.9 / 28.7 |
| acRun | AC | 200 KB raw, 10 s | 600/600 | 3.9 / 5.8 / 7.5 / 23.5 | 48 µs | 11.5 / 33.9 / 28.5 |
| acRun2 | AC | 200 KB raw, 10 s | 600/600 | 4.0 / 4.5 / 4.9 / 5.2 | 106 µs | 11.6 / 30.9 / 7.6 |
| firstRun | not recorded | 50 KB JSON, 5 s | 300/300 | 4.5 / 5.1 / 5.5 / 8.7 | 359 µs | 7.6 / 22.3 / 13.6 |
| batteryRun | battery | 50 KB JSON, 5 s | 300/300 | 8.9 / 10.1 / 11.1 / 11.3 | 684 µs | 8.8 / 31.8 / 23.3 |
| final | battery | 50 KB JSON, 5 s | 300/300 | 6.4 / 9.2 / 10.8 / 11.4 | 478 µs | 28.7 / 28.7 / 18.0 |
| acRun | AC | 50 KB JSON, 5 s | 300/300 | 4.5 / 5.3 / 7.4 / 8.1 | 309 µs | 28.5 / 28.5 / 18.4 |
| acRun2 | AC | 50 KB JSON, 5 s | 300/300 | 4.1 / 4.5 / 4.8 / 5.9 | 357 µs | 7.6 / 28.7 / 10.6 |

- **Nothing was lost or reordered in any run.**
- **The power state explains the battery miss.** On AC, the same test delivers at p50 2.5–3.1 ms against 5.8 ms on battery, and every CPU-bound number moves the same way: Rust send p50 35–47 µs against 70–72 µs, JSON send 309–357 µs against 478–684 µs, and 200 KB frames at p95 4.5–5.8 ms against 8.7–9.4 ms. On AC, 0C meets its target in four of five runs (p95 3.0–3.2 ms).
- **On battery, 0C stays over its target (p95 6.7–6.8 ms).** That's still under a tenth of the 70 ms glass-to-glass budget, so there's no need for the local-WebSocket fallback. Recommendation: keep < 5 ms as the AC criterion, and judge battery by what users see, 0E's glass-to-glass number, measured on both power states.
- **One AC run stalled.** `acRun`, the first run on AC, was slower throughout (p50 3.1 ms against 2.5–2.7 ms in the other four). It had at least one 333 ms gap in which the page received nothing, then caught up, and about 1% of its frames took over 64 ms (p99). One Rust-side send in that run also took 18 ms (p95: 0.14 ms). The cause is unknown. The runner has logged every gap over 50 ms since then, and the four later runs had none (largest gap 20.7–26.1 ms). A third of a second is a visible freeze, so Stage 2's 30-minute stream watches for these.
- **The JS heap is flat.** After a forced full GC, the page retained 1.2 MB before a 60-second raw run and 1.4 MB after it, and ArrayBuffer backing stores stayed at 0.2 MB (`acHeapCheck`). So 3,600 frames (180 MB of payload) left about 0.2 MB behind. Between collections, the post-GC floor (the minimum of each 10-second window) does climb, by about 2–3 MB over a run, in every run on both power states. A forced GC reclaims it, so that's V8's old generation filling up between major collections, not a leak. Stage 2's 30-minute stream repeats the check.
- **How Tauri delivers large messages** (`tauri` 2.12.5, `ipc/channel.rs`). A Channel message over 1 KiB costs an eval plus an IPC fetch back into Rust, so every video frame pays one round trip. Messages within one Channel arrive in order: the JS side reorders them by index.
- **JSON instead of raw bytes (REVIEW B1).** Every frame still arrives. In all five 5-second JSON runs, JSON costs 4.7–9.5× more Rust-side send time than raw bytes in the same run. At p95 it adds 1.3–3.4 ms, except in `acRun`, whose raw run was already slow (5.3 ms, the same as its JSON run). Raw bytes stay the design. B1 is downgraded from a blocker on the strength of these short runs; a 60-second JSON run would firm that up.

## 0D — End-to-end on the fixture

Rust (`phonebridge-scrcpy`) parses the stream, merges config packets, derives the codec string, and sends each decoder-config record in-band, just before the key frame it configures. It also puts the session's SPS/PPS in front of every later key frame, so any key frame can restart the decoder. The page reconfigures `VideoDecoder` on each record, then waits for a key frame.
- The fixture produced 2 sessions (480x1040, then 1040x480), 2 decoder configs and 120 frames, 4 of them key frames, each starting with SPS/PPS. All 120 decoded with 0 errors. The decoded frame size switched at the rotation, and the canvas ended at 1040x480.
- Paced at 30 fps, delivery from Rust to the page took p50 4.3–4.9 ms and p95 6.1–6.5 ms on battery (batteryRun and final), and p50 2.3–2.5 ms and p95 3.3–3.9 ms on AC (acRun and acRun2).
- Two cross-checks pass:
  - Re-encoding every parsed header reproduces the fixture byte for byte (test in `app/src-tauri/src/spikes/mirror.rs`).
  - A `tokio::select!` loop that drops reads stays in sync when it reads through `PacketReader` (test in `crates/scrcpy`).

## 0E — Glass-to-glass latency

This needs the phone and a 240 fps camera (or an on-screen timer). Measured so far, on the PC side only (p50–p95): 2.3–3.9 ms delivery on AC or 4.3–6.5 ms on battery, plus 0.4–1.2 ms decode, then up to one display refresh (≤ 16.7 ms at 60 Hz) to show the frame. The phone side (capture, encode, USB transfer) isn't measured yet. Measure on AC and on battery, since the power state moves the PC side by a few milliseconds.

## 0F — Control ordering

The test sends 1,000 events (300 in the paced run) through a single writer into a loopback socket. An inversion is an event that arrives after one with a higher sequence number. The runs are firstRun, orderingRun, batteryRun, final, acRun and acRun2, in that order.

| Variant | Invokes | Inversions, per run | Max displacement |
|---|---|---|---|
| One async command per event, fire-and-forget | 1000 | 6, 2, 12, 13, 4, 4 | up to 19 |
| One sync command per event, fire-and-forget | 1000 | 4, 4, 5, 3, 3, 2 | up to 14 |
| Batches of 16 events, fire-and-forget | 63 | 96, 48, 16, 48, 64, 32 | up to 207 |
| Serial sender, a burst of 1,000 | 2 | –, 0, 0, 0, 0, 0 | 0 |
| Serial sender, paced (one event every ≥ 4 ms) | 300 | –, 0, 0, 0, 0, 0 | 0 |

(The serial variants were added after the first run, hence the dashes.)

- **The order is lost between the page and Rust**, before the single writer ever sees the events. That happens even for sync commands, which run on the main thread, so a Rust-side queue can't restore it.
- **The serial sender fixes it.** It keeps one invoke in flight and sends whatever queued up meanwhile as one batch, and it gave 0 inversions in every run.
- **The paced runs took 1,548–1,599 ms for 300 events**, about 5.2–5.3 ms per event (~190 events/s). No event had to be batched, so every invoke finished within the ~5 ms before the next event.
- Stage 4 uses this design (DEVELOPMENT_PLAN Stage 4, REVIEW B6).

## 0G — ADB coexistence

`phonebridge-adb` ran against the adb server already running on this machine (Android SDK platform-tools 37.0.1, the same install Android Studio uses):
- `host:version` returned 41, `host:devices-l` listed no devices, and `host:track-devices` sent the initial empty list.
- The same adb.exe process (PID 42004) served every run today. In the final run and every AC run, the runner checked it before the app started and again after the app was killed. PhoneBridge never ran the adb CLI, and the crate's real-server test passes too.
- **Not covered yet:** a run with a device attached and Android Studio actively using the server. `spike_mirror` now records the adb process ids before and after each session for exactly that run.
- A client/server version mismatch can't happen on this path. PhoneBridge doesn't run an adb client binary; it only speaks the protocol.

## 0H — Packaged sidecar

Deferred. It only matters if the installer ships an `adb.exe`: option 1 (build adb from AOSP) or option 4 (bundle Google's binary) in `docs/LICENSING.md` §3.5. That needs a decision first.

## Fixtures

- `fixtures/synthetic-rotation.bin` (527 KB) and `.json`: generated by `scripts/make-synthetic-fixture.mjs` from libx264 output (High 4.0, 30 fps, 2 s portrait then 2 s landscape, one config packet per session, as MediaCodec sends it). ffmpeg decodes all 120 frames of the unwrapped stream. The crate's replay test and the app's byte-for-byte recorder test both use it.
- Real phone recordings (portrait idle, rotation mid-stream, high motion) are pending. `spike_mirror` records to `fixtures/local/` (git-ignored) when given `recordAs`. `fixtures/README.md` describes how to promote a recording into a committed fixture with a test.

## What changed because of these results

- **REVIEW B1 downgraded:** JSON doesn't threaten 60 fps (based on three 5-second runs). Raw bytes stay, for the CPU saving.
- **REVIEW B6 sharpened:** the order is lost before Rust sees the events. Stage 4's control path uses the serial sender.
- **The protocol code exists now:** `crates/adb` and `crates/scrcpy` (156 tests), each reviewed adversarially against the AOSP adb sources and the scrcpy v5.0.1 sources. Stage 2 builds on them (the `[~]` items in the plan).
- **The runner now records the power state, the adb process ids, arrival gaps over 50 ms and the page's heap after a forced GC**, and refuses to write an incomplete run.

## Next steps (these need you)

1. **Plug in the phone** with USB debugging on. The scrcpy server is ready: `scripts/fetch-scrcpy-server.ps1` downloaded v5.0.1 from the official release and checked its SHA-256 (into `app/src-tauri/binaries/`, git-ignored).
2. **Run 0A and live 0D:** `node scripts/run-spikes.mjs --only mirror`.
3. **Record the three fixtures** at reduced size and bit rate, then promote them as `fixtures/README.md` describes. In `npm run tauri:spikes`, open DevTools (right-click → Inspect) and run, for example, `await window.__spikes.mirror({ seconds: 5, maxSize: 480, videoBitRate: 1000000, recordAs: "portrait-idle" })`.
4. **Run 0E** with a 240 fps camera, or an on-screen timer, on AC and on battery.
5. **Run 0G** with the device attached and Android Studio running.
