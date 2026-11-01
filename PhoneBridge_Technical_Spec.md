# PhoneBridge Technical Specification
## Phase 0 → Phase 1 → Phase 2 Implementation Guide

**Date:** September 2026
**Status:** Pre-build planning
**Stack:** Tauri v2 + React + Rust + Kotlin

---

## Table of Contents

1. Project Scaffolding
2. Sidecar Bundling (ADB + scrcpy)
3. Tauri IPC Strategy (Channel API)
4. scrcpy Protocol Integration
5. Phase 0: Validation Tests (2-3 days)
6. Phase 1: USB Mirroring MVP (5-6 weeks)
7. Phase 2: Product Shell (3-4 weeks)
8. Risk Register
9. Decision Log

---

## 1. Project Scaffolding

### 1.1 Prerequisites

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
rustup target add x86_64-pc-windows-msvc  # Windows target

# Node.js (LTS)
# Use nvm-windows or direct installer — Node 20+

# Tauri CLI
cargo install tauri-cli --version "^2"

# Android Studio (for companion app)
# Install via https://developer.android.com/studio
# Ensure Android SDK 35, Kotlin 2.0+, Compose compiler plugin
```

### 1.2 Create the Tauri project

```bash
# Create project
npm create tauri-app@latest phonebridge -- \
  --template react-ts \
  --manager npm

cd phonebridge
```

### 1.3 Project directory structure

```
phonebridge/
├── src/                          # React frontend
│   ├── App.tsx                   # Root — router + layout
│   ├── main.tsx                  # Entry point
│   ├── pages/
│   │   ├── Dashboard.tsx         # Landing: device status + actions
│   │   ├── Mirror.tsx            # Mirror session view
│   │   ├── Files.tsx             # File transfer (Phase 3)
│   │   └── Settings.tsx          # App settings (Phase 2)
│   ├── components/
│   │   ├── mirror/
│   │   │   ├── MirrorCanvas.tsx  # WebCodecs decoder + canvas
│   │   │   ├── ControlBar.tsx    # Back/Home/Recent/Volume buttons
│   │   │   ├── ControlOverlay.tsx# Floating controls in fullscreen
│   │   │   └── LatencyOverlay.tsx# Dev overlay showing T0-T5
│   │   ├── device/
│   │   │   ├── DeviceCard.tsx    # Single device status card
│   │   │   ├── DeviceList.tsx    # All paired devices
│   │   │   └── PairDialog.tsx    # Pairing flow UI
│   │   └── common/
│   │       ├── StatusBar.tsx     # Connection status indicator
│   │       └── ErrorBoundary.tsx
│   ├── hooks/
│   │   ├── useDevice.ts          # Device detection + connection state
│   │   ├── useMirror.ts          # Mirror session lifecycle
│   │   ├── useControl.ts         # Touch/keyboard event translation
│   │   └── useSettings.ts        # Persisted settings
│   ├── lib/
│   │   ├── ipc.ts                # Typed wrappers around invoke/Channel
│   │   ├── decoder.ts            # WebCodecs VideoDecoder manager
│   │   ├── protocol.ts           # Control message types + serialization
│   │   └── coordinates.ts        # Canvas-to-device coordinate math
│   └── styles/
│       └── global.css
│
├── src-tauri/                    # Rust backend
│   ├── src/
│   │   ├── main.rs               # Tauri builder + state init
│   │   ├── lib.rs                # Module declarations
│   │   ├── state.rs              # AppState struct
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── device.rs         # detect_devices, pair, unpair
│   │   │   ├── mirror.rs         # start_mirror, stop_mirror
│   │   │   ├── control.rs        # send_touch, send_key, send_button
│   │   │   ├── screenshot.rs     # take_screenshot
│   │   │   └── settings.rs       # get/set settings
│   │   ├── adb/
│   │   │   ├── mod.rs
│   │   │   ├── client.rs         # Spawn adb commands, parse output
│   │   │   └── device.rs         # Device struct, serial, model name
│   │   ├── scrcpy/
│   │   │   ├── mod.rs
│   │   │   ├── server.rs         # Push JAR, start server process
│   │   │   ├── video.rs          # Read video packets from socket
│   │   │   └── control.rs        # Write control messages to socket
│   │   ├── pairing/
│   │   │   ├── mod.rs
│   │   │   ├── manager.rs        # Pairing session lifecycle
│   │   │   └── store.rs          # Paired device persistence
│   │   └── settings/
│   │       ├── mod.rs
│   │       └── config.rs         # Settings struct + file I/O
│   │
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json          # Permissions for shell, fs, etc.
│   └── binaries/                 # Bundled external binaries
│       ├── adb-x86_64-pc-windows-msvc.exe
│       ├── AdbWinApi.dll         # → copied alongside adb at build time
│       ├── AdbWinUsbApi.dll
│       └── scrcpy-server.jar     # → NOT a sidecar; pushed to device via adb
│
├── companion/                    # Android Kotlin app (separate project)
│   ├── app/
│   │   └── src/main/
│   │       ├── java/com/phonebridge/companion/
│   │       │   ├── MainActivity.kt
│   │       │   ├── PhoneBridgeApp.kt        # Application class
│   │       │   ├── ui/
│   │       │   │   ├── theme/Theme.kt
│   │       │   │   ├── SetupScreen.kt
│   │       │   │   ├── PairingScreen.kt
│   │       │   │   └── StatusScreen.kt
│   │       │   ├── service/
│   │       │   │   └── ConnectionService.kt
│   │       │   ├── pairing/
│   │       │   │   ├── PairingManager.kt
│   │       │   │   └── DeviceKeyStore.kt
│   │       │   └── model/
│   │       │       └── PairedDevice.kt
│   │       ├── res/
│   │       └── AndroidManifest.xml
│   ├── build.gradle.kts
│   └── gradle/
│
├── protocol/                     # Shared documentation
│   ├── pairing.md                # Pairing protocol spec
│   ├── control.md                # Control message format reference
│   └── video.md                  # Video stream format reference
│
├── package.json
├── tsconfig.json
├── vite.config.ts
└── README.md
```

### 1.4 Key Cargo.toml dependencies

```toml
[package]
name = "phonebridge"
version = "0.1.0"
edition = "2021"

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-shell = "2"         # For sidecar (adb) management
tauri-plugin-fs = "2"            # File system access
tauri-plugin-dialog = "2"        # Save/open dialogs
tauri-plugin-store = "2"         # Persisted key-value settings
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["rt", "net", "io-util", "sync", "time"] }
byteorder = "1"                  # Reading big-endian integers from scrcpy stream
uuid = { version = "1", features = ["v4"] }
dirs = "6"                       # Platform-specific directories (Pictures, etc.)
log = "0.4"
env_logger = "0.11"

[build-dependencies]
tauri-build = { version = "2", features = [] }
```

Note: the `scrcpy` crate (scrcpy-protocol, v0.1 on crates.io) tracks scrcpy server v3.3.3. Since scrcpy v4.0 is the latest release (May 2026), the protocol may have minor differences. I recommend hand-writing the packet reader (~80 lines) for now and pinning to a specific scrcpy server JAR version that you've tested. If the `scrcpy` crate updates to v4.0 compatibility, switch to it.

### 1.5 tauri.conf.json — critical settings

```jsonc
{
  "$schema": "https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-config-schema/schema.json",
  "productName": "PhoneBridge",
  "version": "0.1.0",
  "identifier": "com.phonebridge.app",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "windows": [
      {
        "title": "PhoneBridge",
        "width": 420,
        "height": 780,
        "resizable": true,
        "minWidth": 360,
        "minHeight": 640
      }
    ],
    "trayIcon": {
      "iconPath": "icons/tray.png",
      "iconAsTemplate": true
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/icon.ico"
    ],
    "externalBin": [
      "binaries/adb"
    ],
    "resources": [
      "binaries/AdbWinApi.dll",
      "binaries/AdbWinUsbApi.dll",
      "binaries/scrcpy-server.jar"
    ]
  }
}
```

### 1.6 Sidecar naming convention

Tauri requires sidecar binaries to have a target-triple suffix. For Windows x86_64:

```
src-tauri/binaries/adb-x86_64-pc-windows-msvc.exe
```

At runtime, Tauri resolves this to just `adb` for the current platform. ADB's companion DLLs (`AdbWinApi.dll`, `AdbWinUsbApi.dll`) go in `resources` instead — they're not executables, just DLLs that ADB loads at runtime. They need to be in the same directory as `adb.exe`.

The scrcpy server JAR is NOT a sidecar — it's a resource file that gets pushed to the Android device via ADB. It goes in `resources`.

### 1.7 capabilities/default.json

```json
{
  "identifier": "default",
  "description": "Default capabilities for PhoneBridge",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "shell:allow-execute",
    "shell:allow-spawn",
    "shell:allow-stdin-write",
    "shell:allow-kill",
    {
      "identifier": "shell:allow-sidecar",
      "allow": [
        {
          "name": "binaries/adb",
          "sidecar": true,
          "args": true
        }
      ]
    },
    "fs:allow-read-text-file",
    "fs:allow-write-text-file",
    "fs:allow-exists",
    "fs:allow-mkdir",
    "dialog:allow-save",
    "store:default"
  ]
}
```

---

## 2. Tauri IPC Strategy: Channel API

### 2.1 Why not Events

Tauri's official documentation states:

> The event system is not designed for low latency or high throughput situations. See the channels section for the implementation optimized for streaming data.

Events serialize payloads as JSON strings. For our ~50KB H.264 packets at 60 FPS, that's 3MB/s of JSON encoding/decoding overhead on top of the data. Events also don't guarantee ordering under rapid emission — a fast async listener may process events out of order.

### 2.2 Channel API — the correct mechanism

Tauri v2's Channel API is specifically designed for streaming ordered data. It's used internally for download progress, child process output, and WebSocket messages.

**Rust side:**

```rust
use tauri::ipc::Channel;

#[derive(Clone, serde::Serialize)]
#[serde(tag = "type")]
pub enum VideoEvent {
    #[serde(rename = "metadata")]
    Metadata {
        device_name: String,
        width: u32,
        height: u32,
    },
    #[serde(rename = "config")]
    Config {
        #[serde(with = "serde_bytes")]  // sends as binary, not JSON array
        data: Vec<u8>,
    },
    #[serde(rename = "frame")]
    Frame {
        pts: u64,
        keyframe: bool,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
        received_us: u64,  // microsecond timestamp for latency tracking
    },
    #[serde(rename = "stopped")]
    Stopped {
        reason: String,
    },
}

#[tauri::command]
async fn start_mirror(
    serial: String,
    on_event: Channel<VideoEvent>,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let app_state = state.inner().clone();

    // 1. Push scrcpy server, set up ADB forward, start server
    let (mut video_reader, control_sender) =
        setup_scrcpy(&app_state.adb, &serial).await.map_err(|e| e.to_string())?;

    // 2. Store control sender for touch/key commands
    *app_state.control.lock().unwrap() = Some(control_sender);

    // 3. Read metadata
    let meta = video_reader.read_metadata().map_err(|e| e.to_string())?;
    on_event.send(VideoEvent::Metadata {
        device_name: meta.device_name,
        width: meta.width,
        height: meta.height,
    }).map_err(|e| e.to_string())?;

    // 4. Spawn packet reading loop
    // Channel is Clone + Send, so it can move into a thread
    let channel = on_event.clone();
    std::thread::spawn(move || {
        loop {
            match video_reader.read_packet() {
                Ok(packet) => {
                    let event = match packet {
                        VideoPacket::Configuration { data } => {
                            VideoEvent::Config { data }
                        }
                        VideoPacket::Data { pts, keyframe, data, received_at } => {
                            VideoEvent::Frame {
                                pts,
                                keyframe,
                                data,
                                received_us: received_at,
                            }
                        }
                    };
                    if channel.send(event).is_err() {
                        break; // Frontend disconnected
                    }
                }
                Err(e) => {
                    let _ = channel.send(VideoEvent::Stopped {
                        reason: e.to_string(),
                    });
                    break;
                }
            }
        }
    });

    Ok(())
}
```

**Frontend side:**

```typescript
import { invoke, Channel } from '@tauri-apps/api/core';

type VideoEvent =
  | { type: 'metadata'; device_name: string; width: number; height: number }
  | { type: 'config'; data: ArrayBuffer }
  | { type: 'frame'; pts: number; keyframe: boolean; data: ArrayBuffer; received_us: number }
  | { type: 'stopped'; reason: string };

async function startMirror(serial: string, onEvent: (event: VideoEvent) => void) {
  const channel = new Channel<VideoEvent>();
  channel.onmessage = onEvent;

  await invoke('start_mirror', {
    serial,
    onEvent: channel,
  });
}
```

### 2.3 Why this matters for performance

The Channel API uses Tauri's internal streaming mechanism, which is optimized for ordered delivery and binary payloads. With `serde_bytes`, the `Vec<u8>` fields serialize as raw binary rather than JSON arrays of numbers (which would be 3-4x larger on the wire). This means a 50KB H.264 packet transfers as ~50KB of binary, not ~200KB of JSON `[0,1,255,128,...]`.

---

## 3. Phase 0: Validation Tests

### Test matrix (2-3 days total)

| # | Test | Pass criteria | Failure action |
|---|------|--------------|----------------|
| 0A | scrcpy server streams via ADB | `ffplay tcp://127.0.0.1:27183` shows phone screen | Check ADB version, USB debugging, scrcpy server version |
| 0B | WebCodecs in WebView2 | `VideoDecoder.isConfigSupported` returns `supported: true` for `avc1.640029` | Architecture change: decode in Rust with FFmpeg |
| 0C | Channel binary throughput | 60 sends/sec of 50KB payloads, each <5ms on Windows | Try `tauri-wire` binary framing, or local WebSocket fallback |
| 0D | End-to-end smoke | Phone screen renders in Tauri canvas | Debug stage by stage |

### Test 0A: scrcpy raw stream

Download scrcpy v4.0 release from GitHub. It includes the server JAR.

```bash
# Extract scrcpy-server from the release archive
# It's typically named scrcpy-server-v4.0

# Push to device
adb push scrcpy-server-v4.0 /data/local/tmp/scrcpy-server.jar

# Set up port forwarding
adb forward tcp:27183 localabstract:scrcpy

# Start server with raw_stream mode (simplest — no framing headers)
adb shell CLASSPATH=/data/local/tmp/scrcpy-server.jar \
    app_process / com.genymobile.scrcpy.Server 4.0 \
    tunnel_forward=true audio=false control=false \
    cleanup=false raw_stream=true max_size=1920 max_fps=60

# In another terminal — verify with ffplay
ffplay -framerate 60 -probesize 32 -analyzeduration 0 \
    -f h264 tcp://127.0.0.1:27183
```

If ffplay shows your phone screen live, the server works. Measure baseline FPS and visual quality.

Then test WITH frame metadata (what your app will use):

```bash
adb shell CLASSPATH=/data/local/tmp/scrcpy-server.jar \
    app_process / com.genymobile.scrcpy.Server 4.0 \
    tunnel_forward=true audio=false control=true \
    send_frame_meta=true max_size=1920 max_fps=60
```

This time ffplay won't work (the frame headers confuse it), but connecting with a simple script that reads the header bytes will confirm the framed protocol works.

### Test 0B: WebCodecs in Tauri

Scaffold a minimal Tauri app and test WebCodecs availability.
See the code in the previous conversation turn — create a Tauri app,
add the WebCodecs feature detection script, run `npm run tauri dev`.

Expected: "PASS: H.264 hardware decoding supported" on Windows 10/11.

### Test 0C: Channel throughput

Add the throughput test command to the Tauri test app. Measure
on your actual dev machine. Look for:

- Send latency per 50KB packet (target: <5ms)
- Sustained 60 packets/second without backpressure
- No visible memory growth (GC keeping up)

### Test 0D: End-to-end

Combine: Rust reads scrcpy packets → Channel → frontend WebCodecs decode → canvas render. If the phone screen appears in the Tauri window, Phase 0 passes.

---

## 4. Phase 1: USB Mirroring MVP (5-6 weeks)

### Week-by-week breakdown

**Week 1: Rust foundations**

- Set up the Tauri project with the directory structure above
- Implement `adb/client.rs`: spawn adb, parse `adb devices` output
- Implement `scrcpy/server.rs`: push JAR, ADB forward, start server process
- Implement `scrcpy/video.rs`: connect to video socket, read metadata + packets
- Test: packets arrive in Rust, log packet sizes and PTS values

**Week 2: Frontend decoder**

- Implement `lib/decoder.ts`: WebCodecs VideoDecoder lifecycle
- Implement `components/mirror/MirrorCanvas.tsx`: canvas rendering + pointer events
- Implement `hooks/useMirror.ts`: Channel setup, start/stop lifecycle
- Wire up `start_mirror` command with Channel API
- Test: phone screen renders in the Tauri window

**Week 3: Control + navigation**

- Implement `scrcpy/control.rs`: touch, key, text, button messages
- Implement `lib/coordinates.ts`: canvas-to-device coordinate translation
- Implement `hooks/useControl.ts`: pointer event capture, keyboard forwarding
- Implement `components/mirror/ControlBar.tsx`: Back/Home/Recent/Volume buttons
- Test: tap, swipe, type, and navigate on the mirrored screen

**Week 4: Error handling + device management**

- Implement disconnect detection (socket close → reconnect UI)
- Implement `commands/device.rs`: list devices, device info
- Implement `pages/Dashboard.tsx`: device card, Mirror button
- Handle: no device, USB debugging disabled, device unauthorized
- Handle: scrcpy server crash, ADB server not running
- Clean process shutdown on app close

**Week 5: Companion app + pairing**

- Implement the Kotlin companion app (PairingManager, screens, service)
- Implement `pairing/manager.rs` on the Rust side
- Wire up USB-direct pairing (ADB forward → local socket → JSON exchange)
- Implement `pairing/store.rs`: persist paired devices to disk
- Test: full pairing flow — install companion, plug USB, pair, mirror

**Week 6: Polish + latency instrumentation**

- Add latency tracking: timestamp at Rust receive → send to frontend → measure decode + render
- Implement `components/mirror/LatencyOverlay.tsx`: dev overlay showing pipeline timing
- Bundle ADB + scrcpy-server.jar into the Tauri build
- Test Windows installer (NSIS)
- Test on 2-3 different Android devices
- Write setup instructions

### Phase 1 deliverable

User installs PhoneBridge.exe → opens it → plugs in Android phone with companion app → taps Allow → sees Dashboard → clicks Mirror → phone screen appears → controls with mouse and keyboard.

---

## 5. Phase 2: Product Shell (3-4 weeks)

Phase 2 transforms the working prototype into something that feels like a product. No new core capabilities — this is about completeness, reliability, and polish.

### 5.1 What Phase 2 covers

| Area | What gets built |
|------|----------------|
| Device management | List paired devices, remove devices, device names, last seen |
| Settings | Video quality, max FPS, screenshot location, auto-connect, start minimized |
| Application lifecycle | System tray, minimize to tray, auto-reconnect on USB replug |
| Background services | ADB device polling, connection state machine |
| Error handling | Every error state has a user-facing message and recovery path |
| Installer | Proper Windows installer with shortcuts, uninstall, auto-update prep |
| Android companion | Permission guidance, notification channel, auto-start on USB |

### 5.2 Settings architecture

Settings need to persist across sessions. Tauri provides `tauri-plugin-store` which writes a JSON file to the app data directory.

**Settings data model:**

```typescript
// Frontend type — also the shape of the persisted JSON
interface PhoneBridgeSettings {
  // Video
  maxFps: 30 | 60;                    // default: 60
  maxSize: 1080 | 1440 | 1920;       // max dimension, default: 1920
  videoBitRate: number;               // bps, default: 8_000_000
  videoCodec: 'h264';                 // only h264 for Phase 1-2

  // Behavior
  autoConnect: boolean;               // auto-start mirror when device detected, default: false
  startMinimized: boolean;            // start in system tray, default: false
  minimizeToTray: boolean;            // close button minimizes, default: true

  // Paths
  screenshotDir: string;              // default: ~/Pictures/PhoneBridge/
  recordingDir: string;               // default: ~/Videos/PhoneBridge/

  // Developer
  showLatencyOverlay: boolean;        // default: false
}
```

**Rust side — settings commands:**

```rust
use tauri_plugin_store::StoreExt;

#[tauri::command]
fn get_settings(app: tauri::AppHandle) -> Result<PhoneBridgeSettings, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    // Read each field with defaults
    let max_fps = store.get("maxFps")
        .and_then(|v| v.as_u64())
        .unwrap_or(60) as u32;
    // ... etc
    Ok(PhoneBridgeSettings { max_fps, /* ... */ })
}

#[tauri::command]
fn set_setting(app: tauri::AppHandle, key: String, value: serde_json::Value) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    store.set(key, value);
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}
```

Settings are read by the Rust backend when starting a mirror session (maxFps, maxSize, videoBitRate get passed to scrcpy server arguments). The frontend reads them to display the settings UI.

### 5.3 Device management

**Paired device data model:**

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct PairedDevice {
    pub id: String,             // UUID generated during pairing
    pub name: String,           // Device model name ("Pixel 8")
    pub serial: String,         // ADB serial (changes with USB port — use for matching only)
    pub public_key: String,     // Base64 public key from pairing
    pub paired_at: u64,         // Unix timestamp
    pub last_seen: u64,         // Last successful connection
    pub connection_count: u32,  // Number of times connected
}
```

**Storage:** Paired devices are stored in `tauri-plugin-store` as a JSON array under the key `pairedDevices`. The store file lives at `{app_data_dir}/paired-devices.json`.

**Device matching:** When ADB detects a connected device, match it to a paired device by checking if the device's identity (obtained through the pairing socket) matches a stored public key. Don't rely solely on ADB serial numbers — they can change when the device is plugged into a different USB port.

**Unpair flow:**

```rust
#[tauri::command]
fn unpair_device(app: tauri::AppHandle, device_id: String) -> Result<(), String> {
    let store = app.store("paired-devices.json").map_err(|e| e.to_string())?;
    let mut devices: Vec<PairedDevice> = store.get("devices")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    devices.retain(|d| d.id != device_id);

    store.set("devices", serde_json::to_value(&devices).unwrap());
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}
```

### 5.4 Application lifecycle

**System tray:**

```rust
// In main.rs, within the Tauri builder:
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState},
};

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show PhoneBridge", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                // Clean up ADB/scrcpy processes before exit
                cleanup_processes(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
```

**Window close → minimize to tray:**

```rust
// In the Tauri builder setup:
app.get_webview_window("main")
    .unwrap()
    .on_window_event(|event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            // Check settings for minimize_to_tray
            // If true, prevent close and hide instead
            api.prevent_close();
            event.window().hide().unwrap();
        }
    });
```

**Auto-reconnect on USB replug:**

The Rust backend runs a background polling loop that checks `adb devices` every 2 seconds. When a paired device appears:

```rust
fn start_device_polling(app: tauri::AppHandle, state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut known_devices: HashSet<String> = HashSet::new();

        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;

            let current_devices = match state.adb.devices() {
                Ok(d) => d,
                Err(_) => continue,
            };

            let current_set: HashSet<String> = current_devices.iter().cloned().collect();

            // Detect new connections
            for serial in current_set.difference(&known_devices) {
                let _ = app.emit("device:connected", serial.clone());

                // If auto-connect is enabled and device is paired,
                // automatically start mirroring
                if state.settings.auto_connect {
                    if state.is_paired(serial) {
                        // Emit event that frontend can react to
                        let _ = app.emit("device:auto-connect", serial.clone());
                    }
                }
            }

            // Detect disconnections
            for serial in known_devices.difference(&current_set) {
                let _ = app.emit("device:disconnected", serial.clone());
            }

            known_devices = current_set;
        }
    });
}
```

### 5.5 Connection state machine

The app has clear states. Modeling them explicitly prevents bugs.

```rust
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state")]
pub enum ConnectionState {
    /// No device detected
    Disconnected,

    /// Device detected via ADB but not yet paired
    DetectedUnpaired { serial: String, model: String },

    /// Device detected and paired, ready to mirror
    Ready { serial: String, device: PairedDevice },

    /// Mirror session starting (pushing JAR, ADB forward, etc.)
    Connecting { serial: String },

    /// Mirror session active
    Mirroring { serial: String, latency_ms: Option<u32> },

    /// Mirror stopped, device still connected
    Stopped { serial: String, reason: String },

    /// Error state
    Error { message: String, recoverable: bool },
}
```

Frontend subscribes to state changes:

```typescript
// useDevice.ts
const [connectionState, setConnectionState] = useState<ConnectionState>({ state: 'Disconnected' });

useEffect(() => {
    const unlisten = listen<ConnectionState>('connection:state-changed', (event) => {
        setConnectionState(event.payload);
    });
    return () => { unlisten.then(fn => fn()); };
}, []);
```

The Dashboard renders different UI for each state:

- `Disconnected` → "Connect your Android phone via USB"
- `DetectedUnpaired` → "New device detected — Pair"
- `Ready` → Device card with "Mirror" button
- `Connecting` → Spinner with "Starting mirror..."
- `Mirroring` → Redirect to Mirror page (or show mini-preview)
- `Stopped` → "Session ended: {reason}" with reconnect button
- `Error` → Error message with recovery instructions

### 5.6 Error handling matrix

Every error the user can encounter needs a message and a next step.

| Error | Detection | User message | Recovery |
|-------|-----------|-------------|----------|
| No ADB server | `adb devices` fails | "Could not start device detection" | Restart app (auto-starts ADB) |
| No device | No serials in `adb devices` | "No Android device detected. Connect via USB." | Plug in device |
| USB debugging off | Device appears as "unauthorized" | "USB Debugging is not enabled. Follow these steps..." | Link to setup guide in companion app |
| Device unauthorized | `adb devices` shows "unauthorized" | "Allow USB debugging on your phone" | User taps Allow on Android dialog |
| scrcpy push fails | `adb push` returns error | "Could not set up mirroring. Reconnect USB cable." | Retry button |
| scrcpy server crash | Video socket closes unexpectedly | "Mirror session interrupted" | Auto-reconnect if device still present |
| Device unplugged during mirror | Socket read returns EOF + `adb devices` empty | "Phone disconnected" | "Waiting for reconnection..." |
| Companion app not installed | Pairing socket connection refused | "Install the PhoneBridge app on your phone" | QR code / download link |
| Pairing denied | Companion sends `status: "denied"` | "Pairing was denied on the phone" | "Try again" button |
| Port conflict | ADB forward fails | "Could not set up connection (port in use)" | Try a different port, restart ADB |
| Antivirus blocks ADB | ADB process killed or timeout | "Security software may be blocking PhoneBridge" | Link to troubleshooting guide |

### 5.7 Android companion — Phase 2 additions

**Permission guidance flow:**

The companion app's setup screen should detect which permissions are granted and guide the user through each one. Each step only appears if the permission isn't already granted.

```kotlin
// SetupScreen.kt — Phase 2
@Composable
fun SetupScreen(
    isUsbDebuggingEnabled: Boolean,
    isNotificationPermissionGranted: Boolean,
    onRequestNotificationPermission: () -> Unit,
) {
    Column(modifier = Modifier.fillMaxSize().padding(24.dp)) {
        Text("Setup", style = MaterialTheme.typography.headlineMedium)
        Spacer(Modifier.height(24.dp))

        // Step 1: USB Debugging
        SetupStep(
            number = 1,
            title = "Enable USB Debugging",
            description = "Go to Settings → About Phone → tap Build Number 7 times → " +
                          "go back → Developer Options → enable USB Debugging",
            isComplete = isUsbDebuggingEnabled,
        )

        // Step 2: Notification permission (Android 13+)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            SetupStep(
                number = 2,
                title = "Allow Notifications",
                description = "Required to show connection status",
                isComplete = isNotificationPermissionGranted,
                actionLabel = "Grant Permission",
                onAction = onRequestNotificationPermission,
            )
        }

        // Step 3: Ready
        if (isUsbDebuggingEnabled && isNotificationPermissionGranted) {
            Spacer(Modifier.height(32.dp))
            Text(
                "You're all set! Connect your phone to your PC via USB.",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.primary,
            )
        }
    }
}
```

**Notification channel setup:**

```kotlin
// PhoneBridgeApp.kt
class PhoneBridgeApp : Application() {
    override fun onCreate() {
        super.onCreate()

        val channel = NotificationChannel(
            ConnectionService.CHANNEL_ID,
            "PhoneBridge Connection",
            NotificationManager.IMPORTANCE_LOW // No sound, just persistent icon
        ).apply {
            description = "Shows when your phone is connected to a PC"
        }

        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(channel)
    }
}
```

**Auto-start pairing listener on USB connect:**

The companion app registers a `BroadcastReceiver` for USB connection events. When the phone is plugged into a PC, it automatically starts the pairing listener socket.

```kotlin
class UsbConnectionReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action == UsbManager.ACTION_USB_DEVICE_ATTACHED ||
            intent.action == Intent.ACTION_POWER_CONNECTED) {
            // Start the connection service which opens the pairing socket
            val serviceIntent = Intent(context, ConnectionService::class.java)
            context.startForegroundService(serviceIntent)
        }
    }
}
```

### 5.8 Windows installer (NSIS)

Tauri v2 uses NSIS for Windows installers by default. Key configuration:

```jsonc
// In tauri.conf.json → bundle → windows → nsis
{
  "bundle": {
    "windows": {
      "nsis": {
        "displayLanguageSelector": false,
        "installerIcon": "icons/icon.ico",
        "headerImage": "icons/nsis-header.bmp",
        "sidebarImage": "icons/nsis-sidebar.bmp"
      },
      "certificateThumbprint": null,
      "timestampUrl": "http://timestamp.digicert.com"
    }
  }
}
```

**Code signing:** For production distribution, you need a code signing certificate. Without one, Windows SmartScreen will show "Windows protected your PC" to every user. Options:

- **Self-signed (dev only):** Free but users must manually trust it
- **OV certificate:** ~$200-400/year from DigiCert, Sectigo, etc. Removes SmartScreen warning after enough installs
- **EV certificate:** ~$400-600/year, immediately trusted by SmartScreen

For Phase 2, self-signed is fine for testing. Budget for an OV certificate before any public release.

### 5.9 Phase 2 week-by-week

**Week 1: Settings + device management**

- Implement settings page UI (React)
- Implement settings persistence (Rust `tauri-plugin-store`)
- Wire settings into scrcpy server launch arguments
- Implement paired device list, unpair flow
- Implement device info display (model name, serial, last seen)

**Week 2: Application lifecycle**

- System tray integration
- Minimize to tray on close (configurable)
- Auto-reconnect polling loop
- Clean shutdown (kill scrcpy/ADB child processes)
- Window state persistence (size, position)
- Start minimized option

**Week 3: Error handling + companion polish**

- Implement every error state from the error matrix
- Add error recovery flows (retry buttons, setup guidance)
- Companion app: setup screen with permission detection
- Companion app: notification channel + foreground service polish
- Companion app: auto-start pairing listener on USB connect
- Test error scenarios (unplug during mirror, deny pairing, etc.)

**Week 4: Installer + testing**

- Build Windows NSIS installer
- Test install/uninstall/upgrade cycle
- Test on Windows 10 and Windows 11
- Test on 3+ Android devices (different manufacturers)
- Test ADB coexistence with Android Studio
- Build Android APK for sideloading (Play Store comes later)
- Write user-facing setup documentation

### 5.10 Phase 2 deliverable

A polished Windows application with:
- Working installer that bundles ADB and scrcpy-server
- System tray with minimize-to-tray behavior
- Settings that persist across sessions and affect mirror quality
- Paired device management (pair, unpair, auto-reconnect)
- Clear error messages for every failure mode
- Android companion app with guided setup flow
- Latency instrumentation (dev overlay, toggleable in settings)

---

## 6. Decision Log

Decisions made during planning, with rationale. Reference this when the question comes up again.

| # | Decision | Rationale | Revisit when |
|---|----------|-----------|-------------|
| D1 | Decode H.264 on frontend with WebCodecs, not in Rust | Tauri IPC ~200ms for 10MB on Windows (decoded frames). H.264 packets are ~50KB — fast enough for IPC. WebCodecs supported in WebView2 since Edge 94. | Phase 0 test 0B fails |
| D2 | Use Tauri Channel API, not Events, for video streaming | Events are JSON-serialized, not designed for high throughput. Channels are Tauri's recommended mechanism for streaming data. | Never (this is the correct API) |
| D3 | Pure Kotlin for Android companion, not React Native | Companion is 90% native Android APIs. RN adds bridge overhead, larger APK, build complexity for minimal UI benefit. | iOS support becomes a real goal |
| D4 | USB-direct pairing for Phase 1, QR for Phase 5 | QR pairing requires camera permission, QR scanner library, and is unnecessary when USB provides a direct communication path. | Phase 5 (Wi-Fi) |
| D5 | Use scrcpy server JAR as-is, hand-write Rust client | scrcpy server is battle-tested across hundreds of Android devices. The Rust client is ~400 lines of simple socket I/O. The `scrcpy-protocol` crate tracks v3.3.3, not v4.0. | scrcpy-protocol crate updates to v4.0 |
| D6 | ADB as sidecar, scrcpy-server as resource | ADB is an executable (sidecar). scrcpy-server.jar is pushed to the device (resource). DLLs are resources co-located with ADB. | Never |
| D7 | Android 10 (API 29) minimum | AudioPlaybackCapture requires API 29. Devices below Android 10 are a shrinking share. | Never |
| D8 | Connection state as explicit enum | Prevents impossible states, makes every error recoverable, simplifies frontend rendering. | Never |
| D9 | Settings via tauri-plugin-store | Simple JSON persistence, no database, built-in Tauri support. | Settings become complex enough to need a schema migration strategy |
| D10 | ADB-over-TCP for LAN (Phase 5), WebRTC only for Internet (Phase 6) | ADB TCP is proven, simple, low-overhead for LAN. WebRTC adds complexity only justified by NAT traversal. | Phase 5 spike reveals ADB TCP limitations |

---

## 7. Risk Register

| Risk | Phase | Likelihood | Impact | Mitigation |
|------|-------|-----------|--------|------------|
| WebCodecs not available in user's WebView2 | 1 | Low | Critical | Phase 0 test. Fallback: mandate Edge update via installer prerequisite check. WebView2 auto-updates with Edge. |
| scrcpy v4.0 protocol breaks with future Android versions | 1-2 | Medium | High | Pin to a tested scrcpy-server.jar version. Monitor scrcpy GitHub releases. |
| Tauri Channel throughput insufficient for 60 FPS | 1 | Low | High | Phase 0 test. Fallback: local WebSocket between Rust and frontend, or reduce to 30 FPS default. |
| ADB conflicts with Android Studio on user's machine | 2 | Medium | Medium | Don't start a separate ADB server — use the existing one. Test coexistence. |
| Windows Defender blocks ADB sidecar | 2 | Medium | Medium | Code sign the installer. Document exclusion steps. |
| Specific Android devices fail with scrcpy | 1-2 | High | Medium | Test on 5+ devices from different manufacturers. scrcpy has known issues with some Samsung, Xiaomi models — check their issue tracker. |
| Battery drain complaints on Android | 2+ | High | Medium | Implement adaptive FPS (60→10 on idle, pause when minimized). Show battery usage estimate. |
| Users can't find USB debugging setting | 2 | High | Low | Companion app guides them step by step. Popular phones have different paths — document the top 5 manufacturers. |

---

## 8. What's NOT in Phase 1-2

Explicitly out of scope to prevent scope creep:

- File transfer (Phase 3)
- Audio capture and playback (Phase 4)
- Screen recording (Phase 4)
- Wi-Fi connectivity (Phase 5)
- Internet/WebRTC connectivity (Phase 6)
- TURN relay (Phase 7)
- Custom capture engine replacing scrcpy (Phase 7+)
- Google Play Store listing (post-Phase 2, when the product is stable)
- Auto-update mechanism (post-Phase 2)
- Analytics/telemetry (post-Phase 2)
- Multi-language support (post-1.0)
