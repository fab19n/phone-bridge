# PhoneBridge — Stage 1 Results (Validation Spikes)

**Date:** 2026-11-02 · **Plan:** `docs/DEVELOPMENT_PLAN.md`, Stage 1
**Raw data:**
- `docs/stage1-results.json`: the last phone-free run on battery, `final` below (the runner records the power state itself).
- `docs/stage1-results-earlier.json`: the earlier runs the same day. `firstRun`: power not recorded. `orderingRun`: 0F only. `batteryRun`: a full run. `batteryRepeat`: 0C only.
- `docs/stage1-results-ac.json`: five later runs on AC power. `acRun` and `acRun2`: full runs. `acRepeat1`, `acRepeat2` and `acHeapCheck`: 0C only. From `acRepeat1` on, the runner logs arrival gaps over 50 ms; `acHeapCheck` adds the retained-heap check.
- `docs/stage1-results-phone.json`: the phone runs, with the device serial redacted. `mirror`: 0A and live 0D at the default size, frame-rate cap and bitrate (1920 px, 60 fps, 8 Mbps). `recording1` and `recording2`: the first fixture takes. `recording2` redid `recording1`'s `portrait-idle` (70 frames: the screen kept changing; the redo, with 67, still did) and `high-motion` (88 frames: too little motion), and the takes kept from the two runs were later replaced because they showed personal content. `recording3`: a re-record on the Settings → Display page; its `high-motion` take caught no motion and the status bar showed notification icons, so all three were recorded again. `recording4`: the committed fixtures. The runner's per-take check (`takeProblem`) came after `recording3`, so the discarded takes don't carry one. When this page counts phone sessions (twelve in all), a session is one server launch (`mirror`, or one take), and a rotation take's three capture sessions (the JSON's `stream.sessions`) count as one; in 0D and the fixture table, "sessions" are capture sessions.

**Machine:** Intel Core Ultra 7 258V (8 cores), 32 GB RAM, Intel Arc 140V, Windows 11 Pro build 26200, WebView2 156.0.4314.58. **Build:** release, with the `spikes` feature.
**Phone:** Samsung Galaxy A56 5G (SM-A566B), Android 17 (API 37), Exynos 1580, 1080x2340, over USB, for 0A, 0B, 0D and 0G. Everything else runs on fixtures or on loopback.
**Reproduce:** `node scripts/run-spikes.mjs --build --out docs/stage1-results.json` for the phone-free spikes. With a phone attached, `node scripts/run-spikes.mjs --only mirror` (save its output under a new key in `docs/stage1-results-phone.json`, not over `stage1-results.json`), and `node scripts/run-spikes.mjs --record` for the fixtures (`fixtures/README.md`). Keep the window visible while it runs. The runner records AC vs battery, the adb server's process ids and the page's heap after a forced GC, redacts device serial numbers, and won't write an incomplete run.

## Summary

| # | Test | Status | Result |
|---|---|---|---|
| 0A | scrcpy server streams over ADB | ✅ Pass | On Android 17: push over `sync:`, the server over `shell,v2,raw:`, the sockets over `localabstract:`. The cold start took 1.17 s (11 connect attempts; the first 10 found the server not listening yet). The server exited with code 0 when the sockets closed |
| 0B | WebCodecs in WebView2 | ✅ Pass | The phone's encoder output decodes, with the codec string parsed from the SPS: `avc1.64002A` (High 4.2) at 886x1920 and `avc1.64001E` (High 3.0) at 220x480. All 1,189 frames of the twelve phone sessions decoded with 0 errors, at p50 0.4–1.0 ms. `isConfigSupported` accepts every H.264 configuration tested (Baseline and Main 3.1, High 4.0–5.1), and the phone's High 3.0 decodes too |
| 0C | Raw-byte Channel throughput | ✅ Pass on AC · ⚠ over target on battery | 60 × 50 KB/s for 60 s, every frame delivered in order in all nine runs. On AC, p95 3.0–3.2 ms in four of five runs; the fifth was slower throughout (5.3 ms) and had one 333 ms stall. On battery, 6.7–6.8 ms in three runs (target < 5 ms). The heap is flat: 1.2 MB retained before a run, 1.4 MB after |
| 0D | End-to-end | ✅ Pass | Live: the phone's screen showed in the canvas (886x1920). Rotating it to landscape and back gave three sessions; the decoder was reconfigured each time, the canvas followed, and every frame decoded with 0 errors, in three rotation takes |
| 0E | Glass-to-glass latency | ⏳ Needs a 240 fps camera or an on-screen timer | PC side, live from the phone: delivery p50 0.8–2.1 ms on AC (up to 4.0 ms on battery) and decode p50 0.4–1.0 ms, though each session's slowest frame took 11–32 ms to decode; then up to one display refresh. The phone side isn't measured yet |
| 0F | Control ordering | ✅ Pass with a design change | Fire-and-forget invokes reorder, even sync commands. A serial sender (one invoke in flight) gives 0 inversions |
| 0G | ADB coexistence | ✅ Pass with a device · ⏳ with Android Studio open | The 5037 client used the running adb server (protocol 41) without restarting it: the same process (PID 42004) before and after every run that recorded it, including twelve phone sessions. Android Studio itself wasn't running |
| 0H | Packaged sidecar | ⏸ Deferred | Only matters if the installer ships an `adb.exe` (options 1 or 4 in `docs/LICENSING.md` §3.5). That needs a decision first |

**Exit gate: not met yet.** 0A, 0B, 0D and 0F pass, 0C passes on AC, and 0G passes with a device attached. On battery, 0C runs over its target, but at under a tenth of the 70 ms latency budget, so it changes no design decision (see 0C). Still needed:
- 0E, the glass-to-glass latency, with a 240 fps camera or an on-screen timer.
- 0G with Android Studio open.
- The adb distribution decision, for 0H.

## 0A — scrcpy server over ADB

`spike_mirror` ran the pinned scrcpy-server v5.0.1 on the Galaxy A56 (`protocol/video.md` §13): `mirror` at PhoneBridge's default size, frame-rate cap and bitrate (1920 px, 60 fps, 8 Mbps), and the eleven recording takes at 480 px, 60 fps and 1 Mbps:
- Rust checked the jar's SHA-256, pushed it over `sync:`, started the server over `shell,v2,raw:`, and connected the video and control sockets over `localabstract:`. No TCP forward was created, so there's none to leak: after the last two recording runs, with the phone still attached, adb listed no forwards and the phone no reverses.
- **Cold start (REVIEW B9):** the first launch listened only after 1.17 s. Until then adb answered `closed`, and the client retried every 100 ms (11 attempts) before reading the dummy byte. Later launches took 4–8 attempts (0.36–0.79 s).
- The server sent the device name (`SM-A566B`), the codec id, a session packet and a config packet, then frames. It logged one line ("Device: [samsung] samsung SM-A566B (Android 17)") and exited with code 0 once the sockets closed, in all twelve sessions.
- Android 17 was the risk: scrcpy's server relies on hidden Android APIs, and new Android releases have broken it before. v5.0.1 starts and streams video on it, with control enabled. Nothing was sent on the control socket, so input injection, the clipboard and screen power are still untested on Android 17 (Stage 4), and audio was off.
- **No jar or server process was left on the phone** (`protocol/video.md` §14, items 1 and 8, for this device). Checked once after each of the last two recording runs, with the phone still attached: `/data/local/tmp` held no `scrcpy-server.jar`, although every session's server ran from that path, so the jar was pushed and then deleted; and no scrcpy or `app_process` process was running. Only sessions that ended normally were checked, not an app killed mid-stream.

## 0B — WebCodecs in WebView2

- **Support matrix.** `VideoDecoder.isConfigSupported` at 1080x1920 with `optimizeForLatency: true` reports 14 of 16 configurations supported, the same in every run.
  - Supported, with both `prefer-hardware` and `no-preference`: `avc1.42E01F` (Baseline 3.1), `avc1.4D401F` (Main 3.1), `avc1.640028`, `avc1.640029`, `avc1.64002A` and `avc1.640033` (High 4.0 to 5.1), and `av01.0.08M.08` (AV1).
  - Not supported either way: HEVC (`hvc1.1.6.L120.90`). PhoneBridge uses H.264, so this doesn't matter. AV1 is an option for later.
- **Real decode, fixture `synthetic-rotation`.** The fixture is real H.264 from libx264 (High 4.0, 480x1040, then 1040x480), framed exactly as scrcpy v5.0.1 sends it.
  - 120 of 120 frames decoded, 0 skipped, 0 errors.
  - Rust derives the codec string `avc1.640028` from the SPS and merges the config packet into the key frame (REVIEW B3).
  - Paced at 30 fps, decoding one frame takes p50 0.7–0.9 ms and p95 1.2 ms on battery, and p50 0.4–0.5 ms and p95 0.5–0.7 ms on AC.
- **Real phone output.** The Galaxy A56's encoder produced H.264 High: `avc1.64002A` (level 4.2) at 886x1920 and `avc1.64001E` (level 3.0) at 220x480. Rust parsed both codec strings from the SPS, and WebCodecs decoded all 1,189 frames of the twelve phone sessions with 0 errors, at p50 0.4–1.0 ms per frame. Each session's slowest frame took 11–32 ms; in the two shortest takes (11 and 13 frames) and two of the three rotation takes, those slow frames set the p95 (7–23 ms), against 0.5–1.6 ms in the other eight sessions.
  - Neither width is a multiple of 16, so the encoder codes 896 or 224 pixels and the SPS crops the rest. The decoded frames came out at the cropped size.
  - The three recordings are committed fixtures, so CI replays the phone's encoder output (see Fixtures).

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
- **One AC run stalled.** `acRun`, the first run on AC, was slower throughout (p50 3.1 ms against 2.5–2.7 ms in the other four). It had at least one 333 ms gap in which the page received nothing, then caught up, and about 1% of its frames took over 64 ms (p99). One Rust-side send in that run also took 18 ms (p95: 0.14 ms). The cause is unknown. The runner has logged every gap over 50 ms since then, and the four later runs had none (largest gap 20.7–26.1 ms). A third of a second is a visible freeze, so Stage 2's 30-minute stream logs every Rust-to-page delivery over 50 ms. A gap alone doesn't count: on a phone, a still screen sends nothing for seconds (see 0D).
- **The JS heap is flat.** After a forced full GC, the page retained 1.2 MB before a 60-second raw run and 1.4 MB after it, and ArrayBuffer backing stores stayed at 0.2 MB (`acHeapCheck`). So 3,600 frames (180 MB of payload) left about 0.2 MB behind. Between collections, the post-GC floor (the minimum of each 10-second window) does climb, by about 2–3 MB over a run, in every run on both power states. A forced GC reclaims it, so that's V8's old generation filling up between major collections, not a leak. Stage 2's 30-minute stream repeats the check.
- **How Tauri delivers large messages** (`tauri` 2.12.5, `ipc/channel.rs`). A Channel message over 1 KiB costs an eval plus an IPC fetch back into Rust, so every video frame pays one round trip. Messages within one Channel arrive in order: the JS side reorders them by index.
- **JSON instead of raw bytes (REVIEW B1).** Every frame still arrives. In all five 5-second JSON runs, JSON costs 4.7–9.5× more Rust-side send time than raw bytes in the same run. At p95 it adds 1.3–3.4 ms, except in `acRun`, whose raw run was already slow (5.3 ms, the same as its JSON run). Raw bytes stay the design. B1 is downgraded from a blocker on the strength of these short runs; a 60-second JSON run would firm that up.

## 0D — End-to-end

Rust (`phonebridge-scrcpy`) parses the stream, merges config packets, derives the codec string, and sends each decoder-config record in-band, just before the key frame it configures. It also puts the session's SPS/PPS in front of every later key frame, so any key frame can restart the decoder. The page reconfigures `VideoDecoder` on each record, then waits for a key frame.

**Live on the phone:**
- At the default settings the phone's screen showed in the canvas at 886x1920. The screen barely changed, so only 39 frames came in 10 s: scrcpy encodes a frame when the screen changes, plus up to ten 100 ms repeats after it (see below). All decoded with 0 errors. Rust-to-page delivery took p50 0.8 ms and p95 5.0 ms.
- **Rotation (REVIEW B14):** turning the phone to landscape and back gave three sessions (220x480, 480x220, 220x480), each with its own config packet and key frame. The page reconfigured the decoder each time, the canvas followed, and every frame decoded with 0 errors, in all three rotation takes (121, 97 and 93 frames).
- **A still screen goes quiet after about a second.** In `portrait-idle` nothing on the screen moved: the server sent the first frame, repeated it every 100 ms, ten times (each PTS exactly 100,000 µs after the one before), and then sent nothing for the rest of the 4-second take. That's the server's `REPEAT_FRAME_DELAY_US` (`protocol/video.md` §11.4). Those repeats weren't empty: they came straight after the key frame, and the encoder used them to sharpen the still image, so they grew to 2.4 KB before shrinking to 35 bytes. In `rotation-midstream`, the landscape screen settled into ten repeats too, the first 116 ms after the frame before it and the other nine exactly 100 ms apart; by then the picture had stopped changing, so each was 16 bytes and changed no pixel. Then nothing came for 1.35 s until the phone was rotated back. Stage 2 mustn't treat a quiet stream as a dead one.

**On the synthetic fixture:**
- The fixture produced 2 sessions (480x1040, then 1040x480), 2 decoder configs and 120 frames, 4 of them key frames, each starting with SPS/PPS. All 120 decoded with 0 errors. The decoded frame size switched at the rotation, and the canvas ended at 1040x480.
- Paced at 30 fps, delivery from Rust to the page took p50 4.3–4.9 ms and p95 6.1–6.5 ms on battery (batteryRun and final), and p50 2.3–2.5 ms and p95 3.3–3.9 ms on AC (acRun and acRun2).
- Two cross-checks pass:
  - Re-encoding every parsed header reproduces the fixture byte for byte (test in `app/src-tauri/src/spikes/mirror.rs`).
  - A `tokio::select!` loop that drops reads stays in sync when it reads through `PacketReader` (test in `crates/scrcpy`).

## 0E — Glass-to-glass latency

This needs a 240 fps camera (or an on-screen timer). Measured so far, on the PC side only: live from the phone, delivery p50 0.8–2.1 ms on AC and 0.8–4.0 ms on battery, and decode p50 0.4–1.0 ms, though each session's slowest frame took 11–32 ms to decode; with the synthetic fixture, delivery took 2.3–3.9 ms on AC or 4.3–6.5 ms on battery (p50–p95). Then it waits up to one display refresh (≤ 16.7 ms at 60 Hz) to show the frame. The phone side (capture, encode, USB transfer) isn't measured yet. Measure on AC and on battery, since the power state moves the PC side by a few milliseconds.

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
- The same adb.exe process (PID 42004) was running before and after every run that recorded it: `final`, every AC run and the five phone runs (the runner checked it before the app started and after it was killed), `batteryRun`'s adb spike, and each of the twelve phone sessions (`spike_mirror` checked it around each one). The earlier `firstRun`, `orderingRun` and `batteryRepeat` predate the check. PhoneBridge never ran the adb CLI, and the crate's real-server test passes too.
- With the phone attached, `host:devices-l` listed it as `device` (authorized), and its features included `shell_v2`.
- Once, after the phone was unplugged and plugged back in, the adb server listed no device, and still none in the 3 minutes after a re-plug was asked for, although Windows showed the phone's ADB interface as working. Roughly 10–15 minutes later it listed the phone as `device`, from the same server process (PID 42004). Whether the phone was re-plugged again in between isn't known, so what brought it back isn't known either. Since adb can list nothing while the phone is plugged in, Stage 5's "No device" message (the spec's §5.6 error matrix) mustn't assume the cable is out. It can suggest re-plugging or toggling USB debugging, though neither is shown to help yet, and it mustn't restart the user's adb server (REVIEW B10).
- **Not covered yet:** Android Studio itself running alongside. It wasn't open during the phone sessions.
- A client/server version mismatch can't happen on this path. PhoneBridge doesn't run an adb client binary; it only speaks the protocol.

## 0H — Packaged sidecar

Deferred. It only matters if the installer ships an `adb.exe`: option 1 (build adb from AOSP) or option 4 (bundle Google's binary) in `docs/LICENSING.md` §3.5. That needs a decision first.

## Fixtures

- `fixtures/synthetic-rotation.bin` (527 KB) and `.json`: generated by `scripts/make-synthetic-fixture.mjs` from libx264 output (High 4.0, 30 fps, 2 s portrait then 2 s landscape, one config packet per session, as MediaCodec sends it). ffmpeg decodes all 120 frames of the unwrapped stream. The crate's replay test and the app's byte-for-byte recorder test both use it.
- Three phone recordings from the Galaxy A56, at 480 px, 60 fps and 1 Mbps (`node scripts/run-spikes.mjs --record`), 0.76 MB in all, of the Settings → Display page with Do Not Disturb on. They replace earlier takes that showed personal content or caught too little motion.

| Fixture | Size | Sessions | Frames | What it covers |
|---|---|---|---|---|
| `portrait-idle` | 20 KB | 220x480 | 11 | A still screen: the first frame, ten 100 ms repeats, then nothing for the rest of the 4 s take |
| `rotation-midstream` | 123 KB | 220x480, 480x220, 220x480 | 9 + 41 + 43 | Rotation to landscape and back; in landscape, ten repeats (the first 116 ms after the frame before it, the rest 100 ms apart), then 1.35 s of silence before the rotation back |
| `high-motion` | 618 KB | 220x480 | 293 | Scrolling for the whole take, at the 60 fps and 1 Mbps caps |

- Each phone fixture's `.json` holds its expected values, written by `scripts/describe-fixture.mjs` with its own JavaScript parser. `crates/scrcpy/tests/phone_fixtures.rs` checks the crate's header parser and `VideoStream` against them, including a replay that drops reads mid-packet.
- The recordings show the phone's screen and the repository is public, so before committing, every frame was checked for personal content on numbered contact sheets, zooming in to 3× where needed, by the author and six other reviewers. The only thing flagged was the stock person glyph in Samsung's Light/Dark theme preview. `scripts/preview-fixture.mjs --sheets` makes such sheets (at 2×), along with an MP4.

## What changed because of these results

- **REVIEW B1 downgraded:** JSON doesn't threaten 60 fps (based on five 5-second runs). Raw bytes stay, for the CPU saving.
- **REVIEW B6 sharpened:** the order is lost before Rust sees the events. Stage 4's control path uses the serial sender.
- **The protocol code exists now, and has run on a device:** `crates/adb` and `crates/scrcpy` (160 tests), each reviewed adversarially against the AOSP adb sources and the scrcpy v5.0.1 sources, then run against a Galaxy A56 on Android 17. Stage 2 builds on them (the `[~]` items in the plan).
- **The runner now records the power state, the adb process ids, arrival gaps over 50 ms and the page's heap after a forced GC**, refuses to write an incomplete run, redacts device serials wherever they appear (and the repository and home folders), and flags a fixture take that came out wrong (wrong orientation, a missing or unwanted rotation, too little or too much change, or no usable video) while the phone is still at hand.
- **CI replays a phone's encoder output:** three phone fixtures, checked against a second, JavaScript parser (`crates/scrcpy/tests/phone_fixtures.rs`).
- **A quiet stream isn't a dead one:** once the screen stops changing, the phone sends about a second of repeated frames, then nothing until it changes, so Stage 2 must not time a session out on frame silence.

## Next steps (these need you)

1. **Run 0E** with a 240 fps camera, or an on-screen timer, on AC and on battery. Use a moving screen at the default size: the live run so far barely changed (39 frames in 10 s), and moving content is the realistic load.
2. **Run 0G with Android Studio open:** open it with the phone showing in it, then run `node scripts/run-spikes.mjs --only mirror`.
3. **Decide how adb ships** (`docs/LICENSING.md` §3.5), which unblocks 0H.
