# PhoneBridge — Senior Engineering & Product Review

**Reviewed:** 2026-11-01
**Inputs:** `PhoneBridge_Technical_Spec.md` (1,078 lines), `phonebridge-scaffold.tar.gz` (19 source files, ~3,300 LOC: Phase 0 harness, React architecture, Kotlin companion)
**Verdict:** The architecture is sound, and the spec is better than most pre-build plans. **Don't start Phase 1 as written.** Four of its core claims are wrong in ways that would only show up in week 2–5. Fix those during Phase 0. Before that, settle one product question: does Phase 1 need the companion app?

Severity: 🔴 Blocker (it won't work) · 🟠 Major (bugs, rework or security issues) · 🟡 Minor (cleanup)

---

## Part A — Product review

### A1. 🔴 There's no product definition. The spec says *how* but never *for whom* or *why us*.
The spec is purely technical. It never states:
- **Target user.** Developers/QA? Streamers? Office workers who want phone notifications on their PC? Each of these needs a different Phase 1.
- **Positioning against free alternatives.** scrcpy (free, open source, the same engine), Microsoft Phone Link (preinstalled on Windows), Vysor, AirDroid, and GUI wrappers such as QtScrcpy and Escrcpy. As specced, Phase 1–2 is a polished GUI wrapper around scrcpy. That's a crowded and free category.
- **Business model.** Free, freemium, or paid? This affects code signing budget, telemetry, licensing and auto-update.
- **Success metrics.** The spec measures latency (T0–T5) but never sets a target number. Proposed targets: glass-to-glass latency **≤ 70 ms p50 / ≤ 120 ms p95 over USB at 1080p60**; time from install to first mirror **< 3 min**; crash-free sessions **> 99%**.

**Recommendation:** Write a one-page PRD before Stage 2. It should name the user, the single wedge feature competitors don't do well, and the targets above.

> Naming note: the brief says "phone casting tool", but the spec builds **mirroring + remote control**. "Casting" usually means one-way video to a big screen. Settle the term, because it changes marketing and scope (for example, whether control matters at all).

### A2. 🔴 The companion app is on the Phase 1 critical path but adds no Phase 1 value
The Phase 1 deliverable (`PhoneBridge_Technical_Spec.md:557`) requires installing an Android app and pairing it before mirroring. But:
- scrcpy needs **nothing installed** on the phone. It pushes a JAR over ADB.
- ADB already does cryptographic, user-approved pairing (the "Allow USB debugging?" RSA prompt).
- The companion's "pairing" carries no real key material (see B11). It's a second consent dialog that protects nothing.

That means Phase 1 asks the user to sideload an APK and pass two prompts to get what scrcpy gives with one. **Recommendation:** drop the companion from the MVP. Mirror any ADB-authorised device right away. Bring the companion back only when it unlocks something ADB can't: notifications, SMS, call handling, background clipboard sync, or Wi-Fi discovery. This removes about one week of Phase 1 work and most of the onboarding friction.

### A3. 🟠 Onboarding is the real product risk, and the spec treats it as a footnote
Enabling Developer Options and USB debugging is where most non-technical users drop off. Doing it is a prerequisite for using the companion app, so the companion can't guide it the first time. The risk register rates this "High likelihood / Low impact". For anyone who isn't a developer, the impact is **High**: it is the funnel. Build onboarding into the **desktop** app: an animated, manufacturer-specific guide (Samsung, Pixel, Xiaomi, OnePlus, Motorola), live detection of "unauthorized" state, and a prompt to look at the phone.

### A4. 🟡 Cheap wins missing from the Phase 1–2 scope
scrcpy already supports these, so each costs hours, not weeks:
- **Clipboard sync** both ways (`SET_CLIPBOARD` / `GET_CLIPBOARD` control messages). Users expect it right away.
- **Drag-and-drop APK install / file push** (`adb install`, `adb push`). This is most of the "Phase 3 file transfer" value for USB users.
- **Screen-off while mirroring** (`turn_screen_off`) and **stay-awake**. These directly address the battery-drain risk.
- **Audio** (scrcpy ≥ 2.0 on Android 11+). It's Phase 4 in the spec, but the server already does it.

### A5. 🟡 Scope and phasing
Phase 2 holds most of what makes this feel like a product (settings, error states, tray, reconnect). The plan in `docs/DEVELOPMENT_PLAN.md` moves error handling and the state machine earlier, because every later feature depends on them.

---

## Part B — Technical review

### B1. 🔴 `serde_bytes` doesn't make Channel payloads binary. Video would ship as JSON number arrays.
`PhoneBridge_Technical_Spec.md:423-425` says `#[serde(with = "serde_bytes")]` sends `Vec<u8>` as raw binary over a `Channel<VideoEvent>`. A Tauri `Channel<T>` with a serde type is serialized with **serde_json**, and serde_json writes bytes as `[0,1,255,...]`. The frontend code gives this away: it types `data` as `number[] | ArrayBuffer` and handles both (`phase0-tests/src/App.tsx:13-14`, `hooks/useMirror.ts`). A 50 KB packet becomes ~180 KB of JSON, which then has to be parsed on the JS main thread 60 times a second.

**Fix:** use two channels. One is a typed JSON `Channel<VideoEvent>` for metadata, config-changed and stopped events. The other carries video as **raw bytes**: send `tauri::ipc::InvokeResponseBody::Raw(Vec<u8>)` (or `tauri::ipc::Response`) so it arrives as an `ArrayBuffer`. Put a small fixed binary header in front (`u8 flags | u64 pts | u64 recv_us`) and parse it in JS with a `DataView`. **Phase 0 test 0C currently measures the wrong path.** Rewrite it to test raw-byte channels.

### B2. 🔴 Test 0C measures the time to call `send()`, not delivery
`phase0-tests/src-tauri/src/main.rs:314-329` times `on_event.send()` on the Rust side. That's just an enqueue. The pass criteria need **Rust-receive → JS-onmessage latency**. Fix: stamp `recv_us` in the payload and compute `performance.timeOrigin + performance.now()` minus that in JS. Report p50/p95/p99 and JS heap growth over 60 s, not 5 s.

### B3. 🔴 The SPS/PPS config packet is never handled correctly, so the decoder gets no parameter sets
- Phase 0 (`App.tsx:291-309`) configures the decoder on the `config` event and then **discards the config bytes**.
- The architecture version (`useMirror.ts`, `config` case) feeds SPS/PPS as a separate `key` chunk with `pts=0`. A chunk with no slice data produces no frame and can put Chromium's decoder into an error state.

**Fix:** do what the official scrcpy client does. Hold the config packet and **prepend it to the next keyframe** before decoding. Also **derive the codec string from the SPS** (`avc1.` + profile_idc + constraint flags + level_idc, as hex) instead of hard-coding `avc1.640029`. Many phones encode Baseline or Main profile, or a level above 4.1 at 1920 px.

### B4. 🔴 The sidecar DLLs won't sit next to `adb.exe` in the installed app
`tauri.conf.json` (`spec:234-241`) puts `adb` in `externalBin` and the DLLs in `resources` as `binaries/AdbWinApi.dll`. Tauri installs sidecars next to the main exe, but keeps resource paths relative (`$INSTDIR\binaries\...`). `adb.exe` then fails to start with a missing-DLL error, **but only in the installed build**, never in dev. Fix: use the resources *map* form (`"binaries/AdbWinApi.dll": "./"`), or ship `adb.exe` and its DLLs together as resources and launch them from Rust with `resource_dir()`. Verify this in Phase 0 (the new test 0H in the plan), not in week 6.

### B5. 🟠 Dropping delta frames corrupts the picture
`decoder.ts` and the Phase 0 harness drop non-keyframes when `decodeQueueSize > 3`. In H.264, later P-frames reference the dropped frames, so the result is smearing and green blocks until the next IDR. scrcpy's default IDR interval can be **many seconds**. Fix: once you drop, keep dropping *until the next keyframe*. Then ask for one immediately with scrcpy's `RESET_VIDEO` control message, or set a short `i-frame-interval` via `video_codec_options`. Better still, render only the *latest* decoded frame and let the decoder queue drain. A healthy pipeline rarely needs to drop.

### B6. 🟠 Control inputs can arrive out of order
`MirrorCanvas.tsx` sends every pointer event as a separate, un-awaited `invoke('send_touch')`. Tauri runs async commands concurrently on a thread pool, so `DOWN, MOVE, MOVE, UP` can reach the socket as `DOWN, UP, MOVE`. The result is stuck touches and broken swipes. Fix: each command should only **push into one `mpsc` queue** owned by a single writer task that serialises scrcpy control messages. Alternatively, use one long-lived control Channel. Add a monotonic sequence number so you can check ordering in tests.

### B7. 🟠 Touch messages are built in the wrong coordinate space and miss required fields
The scrcpy `INJECT_TOUCH_EVENT` message expects the position **plus the video frame size it was computed against** (`screen_width/height`), a pointer id, pressure and action buttons. The server rejects or rescales events whose frame size doesn't match the current one, for example during rotation. `coordinates.ts` maps to a "device size" from metadata that goes stale after the first rotation. It also ignores the `object-fit: contain` letterbox, so taps drift off-target when the window aspect doesn't match. Fix: map to the **current decoded frame size**, account for the letterbox rect, and send that frame size with every event.

### B8. 🟠 Stale-closure bug: the mirror never reaches `active`
`useMirror.ts` checks `status === 'configuring'` inside a callback created when `status` was `'idle'`, so it never switches to `'active'`. Anything gated on `active` (input capture, UI) stays off. The Phase 0 harness has the same problem with `metadata?.w` (`App.tsx:298`), so it always configures 1920×1080. Fix: keep session state in a ref or a reducer, and preferably let Rust own it (see B12).

### B9. 🟠 The scrcpy server is launched fragilely and never cleaned up
In `phase0-tests/src-tauri/src/main.rs:143-191`:
- With `tunnel_forward`, the `adb forward` port **accepts the TCP connection before the server is listening**, then closes it. A fixed `sleep(500ms)` will fail on slow phones. Do what scrcpy does: connect, try to read the dummy byte, and retry about 100 × 100 ms.
- The fixed port `27183` and the forward are never removed (`adb forward --remove`). Restarting the app or running two sessions collides. Pick a free port per session.
- The server child is bound to `_server_process` and dropped. It is never killed, and its piped stdout/stderr are **never drained**, so a chatty server can block on a full pipe. Keep the `Child`, drain both pipes into the log, and kill it on stop or exit.
- There is **no `stop_mirror`** anywhere on the Rust side, although `ipc.ts` calls it.
- Windows: `std::process::Command` opens a **console window for every adb call** in release builds. Set `CREATE_NO_WINDOW` (`creation_flags(0x08000000)`).
- `Cargo.toml` (`spec:175-188`) lacks `rand` and `serde_bytes`, which the code uses. It won't compile.

### B10. 🟠 Bundling ADB can break Android Studio. "Use the existing server" isn't a strategy.
If PhoneBridge's `adb` client version differs from the running ADB server (Android Studio, scrcpy, Samsung tools), adb prints "server version doesn't match this client; killing..." and **restarts the server**. That breaks the developer's IDE session, and then the IDE restarts it back. Decide on one of these:
(a) Talk to the ADB server **directly over its TCP protocol (port 5037)**. This works with any server version. Spawn your bundled adb only when no server is running.
(b) Run a **private server on a different port** (`ANDROID_ADB_SERVER_PORT`). This is isolated, but two servers then compete for the USB device, which causes problems on Windows.
(c) Prefer the user's adb on PATH, if one exists.
I recommend (a) plus `host:track-devices` for push-based device events. That replaces the 2-second `adb devices` polling (`spec:742-782`), which also blocks a tokio worker on a synchronous process spawn.

### B11. 🟠 Companion pairing has no security properties (and a manifest crash)
- `PairingManager.kt` sends a **random UUID as the "public key"**. There's no challenge, no signature and no key agreement. Anything that can talk to the socket can "pair".
- Android **abstract-namespace sockets have no access control**. Any app on the phone can bind `phonebridge-pair` first and impersonate the companion. On the PC side, any local process can connect to the forwarded TCP port.
- On Android 14+, `FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE` requires at least one prerequisite runtime permission or condition (e.g. USB, Bluetooth or `CHANGE_NETWORK_STATE`). Without it, `startForeground` throws a `SecurityException`.
- `UsbConnectionReceiver` (`spec:936-945`) won't fire. `ACTION_USB_DEVICE_ATTACHED` is for phone-as-USB-*host*, `ACTION_POWER_CONNECTED` can't reach manifest receivers since Android 8, and starting a foreground service from the background is blocked on Android 12+.
- `androidx.security:security-crypto` / `MasterKeys` is deprecated. Use DataStore with a Keystore-wrapped key.

This all supports A2: defer the companion. When it comes back, design pairing properly: Ed25519 identity keys, a short code shown on both screens, and authenticated key exchange (Noise XX or similar). Document it in `protocol/pairing.md`. That file is referenced but doesn't exist.

### B12. 🟠 State is split between Rust and React
The spec defines `ConnectionState` in Rust (good, D8). The scaffold also keeps a separate `MirrorStatus` state machine in `useMirror.ts`, and the Dashboard gets *events* (`device:connected`) alongside *state* (`connection:state-changed`). Make Rust the **single source of truth**: one `SessionManager` actor owns devices, sessions and transitions, and emits a full state snapshot on every change. React only renders it.

### B13. 🟠 Over-broad capabilities: the WebView can run any adb command
`capabilities/default.json` (`spec:258-289`) grants `shell:allow-execute`, `allow-spawn` and the adb sidecar with `"args": true`. A single XSS (for example, a device name rendered unsafely) then gives arbitrary `adb shell` on the user's phone. The frontend doesn't need shell access at all, because Rust spawns adb. **Remove all `shell:*` permissions from the WebView**, scope `fs` to the app dirs, and set a strict CSP.

### B14. 🟡 Smaller correctness issues
- scrcpy PTS is in **microseconds**, but the code divides by 1000, treating it as nanoseconds (`decoder.ts`, `App.tsx:325`). That's harmless for decoding but breaks latency math.
- The spec's minimize-to-tray code ignores the `minimizeToTray` setting and always hides (`spec:731-736`). The `state.settings.auto_connect` reference assumes settings live in `AppState`, but they live in the store.
- The device store is named `paired-devices.json`/key `pairedDevices` (`spec:650`) but the code uses key `devices` (`spec:660`).
- `onWheel` calls `preventDefault()` in React's passive wheel listener, which has no effect. Attach a native listener with `{passive:false}`. Also, `import { invoke }` sits at the bottom of `MirrorCanvas.tsx`.
- Keyboard: `Escape`→Back works, but there's no Ctrl+V clipboard paste, no meta-state for shortcuts, and every printable key goes through `INJECT_TEXT`, which is slow and breaks in some IMEs.
- **Rotation:** scrcpy sends a new config packet and size when the device rotates. The decoder has to re-read the SPS, and the canvas and coordinate mapping have to follow. Neither the spec nor the tests cover this.
- D7 says the min API is 29 "for AudioPlaybackCapture". scrcpy's audio capture needs **Android 11 (API 30)**, and mirroring alone works far below 29. Re-justify the floor (for the desktop app it should be whatever scrcpy supports).

### B15. 🟡 Spec and scaffold housekeeping
- The table of contents (`spec:10-20`) lists sections that don't exist (*Sidecar Bundling*, *scrcpy Protocol Integration*), and the numbering doesn't match the body.
- Test 0B says "See the code in the previous conversation turn" (`spec:481`). The spec came from a chat session, and that content is missing.
- `protocol/pairing.md`, `control.md` and `video.md` are referenced but missing. `control.md` is important: it's the scrcpy control message format you're hand-writing.
- The tarball has junk directories literally named `{phase0-tests/src-tauri/src,phase0-tests/src,companion/...}`. A `mkdir -p {a,b}` ran in a shell without brace expansion. Delete them.
- The scaffold is **code fragments, not a buildable project**: there's no `package.json`, `Cargo.toml`, `tauri.conf.json`, `settings.gradle.kts`, Gradle wrapper, `res/`, `proguard-rules.pro`, `main.tsx`, or the missing components (`LatencyOverlay`, `useControl`, `protocol.ts`, `Settings`, `Files`).
- **No testing strategy and no CI.** The cheapest high-value fix: **record a real scrcpy stream to a file once**, then replay it in Rust unit tests (parser), a JS decoder test (frames out), and a headless e2e test. No phone needed in CI.
- **Verify the scrcpy version.** The server's first argument must match the JAR's version string *exactly* (`"4.0"` in the spec). Confirm it against the JAR you download, and keep it in one constant. → **Resolved in Stage 0:** pinned to **v5.0.1** (see B16).
- **Licensing:** scrcpy-server is Apache-2.0: ship the licence text and scrcpy's copyright lines (v5.0.1 has no NOTICE file; re-check on each bump). ADB platform-tools redistribution is under the Android SDK licence terms. Many tools, including scrcpy's own Windows release, bundle it, but confirm before a commercial release. Details: `docs/LICENSING.md`.

### B16. 🔴 The spec and Phase 0 harness parse the pre-v4 wire format *(added in Stage 0)*
The spec targets scrcpy v4.0, but its packet parsing (`spikes/phase0/src-tauri/src/main.rs:202-264`) is the **v3.x** protocol. v4.0 changed the format, and v5.0.1 (latest stable, 2026-10-08, now pinned) keeps the v4 format:
- The codec meta is now **only a 4-byte codec id**. Width and height moved to a new **12-byte session packet** (bit 63 set), sent at stream start and again on every rotation or resize.
- The media header flags shifted: **bit 62 = config, bit 61 = key frame**, and the PTS is 61 bits.
- The option `send_codec_meta` was renamed to `send_stream_meta`.

A v3 parser misreads every config packet as a key frame and every session packet as a config packet. The harness would hang or decode garbage on the first packet. Also, the session packet makes rotation handling explicit (helping B14): reconfigure the decoder and coordinate mapping when it arrives. The full layout is in `protocol/video.md` §7–9.

---

## Part C — What's good (keep it)

- **WebCodecs in the WebView rather than decoding in Rust** (D1) is the right call: it gives GPU decode for free, and IPC carries ~50 KB compressed packets instead of MB-sized frames.
- **Channels over Events** (D2) is correct. It just needs raw-byte payloads (B1).
- **Using the scrcpy server as-is and hand-writing the client** (D5) is pragmatic and de-risks device compatibility.
- An explicit **connection state enum** (D8), the **error matrix** (§5.6), and a **"not in scope" list** (§8) are signs of a disciplined plan.
- **Phase 0 validation before building** is the right instinct. The tests just need to measure the right things (B1, B2, plus the new 0E–0H).

---

## Part D — Open questions for the product owner

1. Who is the primary user: developers/QA, general consumers, or streamers/presenters?
2. What is the one wedge feature that beats scrcpy + Phone Link for that user?
3. Free, freemium, or paid? (This drives the code-signing, telemetry and update decisions.)
4. Is "casting" (one-way view) or "mirroring + control" the core promise?
5. Is Windows-only acceptable through 1.0, or is macOS on the roadmap? (macOS changes codec, signing and notarisation work.)
6. Do you agree to drop the companion app from the MVP (A2)?
