# scrcpy video stream and connection protocol (pinned: v5.0.1)

Status: Stage 0 deliverable (DEVELOPMENT_PLAN.md, "Write `protocol/video.md`").
Pinned server: `scrcpy-server-v5.0.1`, SHA-256 `764eb6f79811d5211fe9df341120882ba9994c7a61b897d7bf3fb662e53bc536`
(from the release asset `SHA256SUMS.txt` @ v5.0.1; matches the GitHub API asset digest).
Written 2026-11-01 from the sources at git tag **v5.0.1** unless a different tag is named.

Source path abbreviations used below (all at tag `v5.0.1`, base `https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/`):

| Short name | Path |
|---|---|
| `Options.java` | `server/src/main/java/com/genymobile/scrcpy/Options.java` |
| `Server.java` | `server/src/main/java/com/genymobile/scrcpy/Server.java` |
| `CleanUp.java` | `server/src/main/java/com/genymobile/scrcpy/CleanUp.java` |
| `DesktopConnection.java` | `server/src/main/java/com/genymobile/scrcpy/device/DesktopConnection.java` |
| `Streamer.java` | `server/src/main/java/com/genymobile/scrcpy/device/Streamer.java` |
| `SurfaceEncoder.java` | `server/src/main/java/com/genymobile/scrcpy/video/SurfaceEncoder.java` |
| `CaptureControl.java` | `server/src/main/java/com/genymobile/scrcpy/video/CaptureControl.java` |
| `ScreenCapture.java` | `server/src/main/java/com/genymobile/scrcpy/video/ScreenCapture.java` |
| `Controller.java` | `server/src/main/java/com/genymobile/scrcpy/control/Controller.java` |
| `ControlMessage.java` | `server/src/main/java/com/genymobile/scrcpy/control/ControlMessage.java` |
| `ControlMessageReader.java` | `server/src/main/java/com/genymobile/scrcpy/control/ControlMessageReader.java` |
| `build.gradle` | `server/build.gradle` |
| `server.c` | `app/src/server.c` |
| `scrcpy.c` | `app/src/scrcpy.c` |
| `demuxer.c` | `app/src/demuxer.c` |
| `packet_merger.c` | `app/src/packet_merger.c` |
| `adb.c` / `adb_tunnel.c` | `app/src/adb/adb.c`, `app/src/adb/adb_tunnel.c` |
| `meson.build` | `app/meson.build` |
| `scrcpy.1` | `app/scrcpy.1` |
| `develop.md` | `doc/develop.md` |

> **Do not implement the v3.x layout** used by the original spec and its Phase 0 scaffold harness (CONFIG = bit 63, KEY = bit 62, a 12-byte codec meta). v5.0.1 uses the v4.0+ layout in sections 7–9.

---

## 1. Overview: byte sequence on the video socket (tunnel_forward=true, all defaults)

```
video socket (device -> client), in order:
  [1]  dummy byte 0x00            only in forward mode; on the FIRST socket opened (video if enabled)
  [64] device name                on the first socket; UTF-8, NUL padded
  [4]  codec id (u32 BE)          e.g. 0x68323634 "h264" (never 0/1 on video: a video config
                                  error shows up as EOF here instead; see section 7)
  [12] session packet             MSB set; width, height
  [12] media header + N bytes     config packet (SPS/PPS for H.264), CONFIG flag, no PTS
  [12] media header + N bytes     key frame (IDR)
  [12] media header + N bytes     ...more frames...
  [12] session packet             after rotation / resize / RESET_VIDEO (new capture session)
  [12] media header + N bytes     new config packet
  [12] media header + N bytes     new key frame
  ...
```

All multi-byte integers are **big-endian** (Java `DataOutputStream`/`ByteBuffer` defaults; the client reads them with `sc_read32be`/`sc_read64be` in `demuxer.c`).

---

## 2. Push path

| Item | Value | Source |
|---|---|---|
| Device path | `/data/local/tmp/scrcpy-server.jar` | `server.c`: `#define SC_DEVICE_SERVER_PATH "/data/local/tmp/scrcpy-server.jar"` |
| Push argv | `adb -s <serial> push <local> /data/local/tmp/scrcpy-server.jar` | `adb.c` `sc_adb_push`: `{adb, "-s", serial, "push", local, remote}` |
| Asset name | `scrcpy-server-v5.0.1`, a bare file with no `.jar` extension; rename it on push | GitHub release v5.0.1 |

The server finds its own path from `CLASSPATH`: `Server.SERVER_PATH = classPaths[0]` (`Server.java`). `CleanUp.unlinkSelf()` deletes that file with `File.delete()` at the **start** of the cleanup process (`CleanUp.java`). So the JAR is gone from the device once the server is running (with `cleanup=true`, the default). PhoneBridge must **push again on every launch** and cannot cache the device copy. (Whether the delete happens on every Android version is UNVERIFIED. The code path is unconditional in `CleanUp.java`, but failures are only logged.)

Minimum Android: `minSdkVersion 21` (`build.gradle`). Audio needs API 30+ (`doc/audio.md` @ v5.0.1).

---

## 3. scid and the adb forward tunnel

### 3.1 scid
- The client generates it as `sc_rand_u32(&rand) & 0x7FFFFFFF` (`scrcpy.c`). The code comment says it uses only 31 bits "to avoid issues with signed values on the Java-side".
- It goes to the server as `scid=%08x` (8 lowercase hex digits; `server.c`).
- The server parses it with `Integer.parseInt(value, 0x10)`. The default is `-1` ("31-bit non-negative value, or -1"), and values below -1 throw (`Options.java`). A value with bit 31 set would overflow `parseInt` and throw `NumberFormatException`. This is why it is limited to 31 bits.

### 3.2 Socket name
- Client: `SC_SOCKET_NAME_PREFIX "%08x"` with `SC_SOCKET_NAME_PREFIX = "scrcpy_"`, so the name is `scrcpy_0a1b2c3d` (`server.c`).
- Server: `SOCKET_NAME_PREFIX = "scrcpy"`, plus `String.format("_%08x", scid)`. If `scid == -1`, the name is just `scrcpy` (`DesktopConnection.java`).
- It is an abstract-namespace Unix socket: `localabstract:scrcpy_%08x`.

### 3.3 Forward setup and teardown

| Step | argv | Source |
|---|---|---|
| Create | `adb -s <serial> forward tcp:<port> localabstract:scrcpy_<scid8hex>` | `adb.c` `sc_adb_forward` (`"tcp:%" PRIu16`, `"localabstract:%s"`) |
| Remove | `adb -s <serial> forward --remove tcp:<port>` | `adb.c` `sc_adb_forward_remove` |

- The official client tries `adb reverse` first and falls back to forward ("'adb reverse' failed, fallback to 'adb forward'") unless `--force-adb-forward` is given (`adb_tunnel.c`). **PhoneBridge always uses forward** and passes `tunnel_forward=true`.
- Port selection: the default range is `27183..27199` (`meson.build`: `DEFAULT_LOCAL_PORT_RANGE_FIRST '27183'`, `..._LAST '27199'`). `adb_tunnel.c` tries each port in turn until `adb forward` succeeds.
- **The official client removes the forward as soon as all sockets are connected** (`server.c` calls `sc_adb_tunnel_close(...)` right after connecting, and again on failure). The TCP connections already set up keep working without the forward rule. So the port is only held for the length of the handshake.

---

## 4. Server launch command line

### 4.1 Base command (`server.c`, `execute_server`)
```
adb -s <serial> shell CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 5.0.1 [key=value ...]
```
- `/` is an unused argument required by `app_process` (`server.c` marks it as unused).
- **argv[0] for the server must be exactly `5.0.1`.** `Options.java` does `if (!clientVersion.equals(BuildConfig.VERSION_NAME))` and throws `IllegalArgumentException` ("The server version (...) does not match the client (...)"). `VERSION_NAME` = `versionName "5.0.1"` (`build.gradle`). There is no `v` prefix.
- Options are `key=value` pairs in any order (`develop.md`). An unknown key is only warned about, not rejected (`Options.java` default branch). **A typo in a key fails silently.** This is how the v3 key `send_codec_meta` behaves on v5.
- The official client rejects values with shell-special characters (`validate_string()` in `server.c`). PhoneBridge must do the same because the command line passes through `adb shell`.

### 4.2 Options PhoneBridge uses (all verified in `Options.java` @ v5.0.1)

| Key | Server default | PhoneBridge value | Notes |
|---|---|---|---|
| (argv[0]) | — | `5.0.1` | Must match exactly |
| `scid` | `-1` | random 31-bit, `%08x` | Hex-parsed |
| `log_level` | `DEBUG` (server field default) | `info` | The official client always sends it (`server.c`); its own default is `SC_LOG_LEVEL_INFO` (`options.c`) |
| `tunnel_forward` | `false` | `true` | Required. Without it the server tries to *connect* (reverse mode) and no dummy byte is sent |
| `video` | `true` | (omit) | |
| `audio` | `true` | `false` for Stage 0/1 | Audio needs API 30+. If `audio=true` the client **must** open the audio socket, or the server blocks in `accept()` and the control socket is never accepted |
| `control` | `true` | (omit; keep true) | The control socket is opened last: second with PhoneBridge's `audio=false`, third only with `audio=true` (section 5.1) |
| `video_codec` | `h264` | (omit) or `h264` | Also `h265`, `av1` |
| `video_bit_rate` | `8000000` | from settings | int, bits/s |
| `max_size` | `0` (unlimited) | from settings (e.g. 1920) | Limits the larger dimension. The final size is aligned to the encoder's width/height alignment (`SurfaceEncoder.java`), so it may not equal `max_size` exactly. Always take dimensions from the session packet |
| `max_fps` | `0` (unlimited) | from settings | Parsed as float |
| `video_codec_options` | `null` | optional, e.g. `i-frame-interval:int=2`? | Syntax `key[:type]=value[,...]`, types `int` (default), `long`, `float`, `string` (`scrcpy.1`). A user `i-frame-interval` overrides the server default in the `MediaFormat` (codec options are applied last in `createFormat`; see section 11.4) |
| `video_encoder` | `null` | omit | |
| `send_device_meta` | `true` | (omit) | 64-byte name |
| `send_frame_meta` | `true` | (omit) | 12-byte headers. **Never disable** |
| `send_dummy_byte` | `true` | (omit) | Needed for the race fix (section 5) |
| `send_stream_meta` | `true` | (omit) | Codec id + session packets. The v3 name was `send_codec_meta`; **do not send that** |
| `raw_stream` | n/a | **never** | `true` sets all four `send_*` to false |
| `cleanup` | `true` | (omit) | Restores device settings after exit (section 12) |
| `power_on` | `true` | (omit) | |
| `stay_awake` | `false` | from settings | Restored by cleanup |
| `show_touches` | `false` | from settings | Restored by cleanup |
| `power_off_on_close` | `false` | from settings | |
| `clipboard_autosync` | `true` | (omit) | Control-side; see `protocol/control.md` |
| `downsize_on_error` | `true` | (omit) | On a capture/encode error, `prepareRetry` (`SurfaceEncoder.java`): after the first frame, up to `MAX_CONSECUTIVE_ERRORS = 3` retries at the same size (50 ms sleep, even with `downsize_on_error=false`); then, if not yet applied, the encoder-capability constraints (`withCapabilities(caps)`), which can change the size **even after the first frame**. Only the `MAX_SIZE_FALLBACK` step-down `{2560, 1920, 1600, 1280, 1024, 800}` is limited to errors before the first frame. Every retry is a new session: **session packet + config + key frame mid-stream**, same or smaller size. Read the session packet |
| `keep_active` | `false` | omit | |

Example Stage 0 command:
```
adb -s R58M123ABC shell CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 5.0.1 scid=1a2b3c4d log_level=info tunnel_forward=true audio=false video_bit_rate=8000000 max_size=1920
```

The `adb shell` process stays alive for the whole session. Its stdout/stderr carry the server log (`[server] INFO: ...`, errors, and the version-mismatch exception).

---

## 5. Socket open order, dummy byte, and the forward-tunnel race

### 5.1 Server side (`DesktopConnection.java`, `open()`, forward branch)
```java
try (LocalServerSocket localServerSocket = new LocalServerSocket(socketName)) {
    if (video)   { videoSocket   = accept(); if (sendDummyByte) { write(0); sendDummyByte = false; } }
    if (audio)   { audioSocket   = accept(); if (sendDummyByte) { write(0); sendDummyByte = false; } }
    if (control) { controlSocket = accept(); if (sendDummyByte) { write(0); sendDummyByte = false; } }
}   // LocalServerSocket closed here; accepted sockets stay open
```
(Paraphrased structure. The real code calls `xxxSocket.getOutputStream().write(0)`.)

- Accept order: **video, then audio, then control**, skipping any that are disabled.
- **Exactly one dummy byte, value `0x00`, on the first accepted socket** (video when video is enabled). It is written *immediately after that accept*, before the next accept. The client can read it before opening the next socket.
- Reverse mode sends no dummy byte.
- The listening socket is closed after the last accept. A late or extra connection is refused.

### 5.2 Why the race happens
With `adb forward`, the local adb server **accepts the TCP connection on `tcp:<port>` straight away**, whether or not anything is listening on the device's `localabstract:` socket. If the server JVM has not yet created its `LocalServerSocket`, adb closes the connection right after. `connect()` succeeds and the first `read()` returns EOF (0 bytes) or a reset. `server.c` explains this in its comment on `connect_and_read_byte`: the connect alone can succeed without a listening server, so it reads one byte to confirm.

### 5.3 How the official client retries (`server.c`, `connect_to_server`)
```c
do {
    sc_socket socket = net_socket();
    if (socket != SC_SOCKET_NONE) {
        bool ok = connect_and_read_byte(&server->intr, socket, host, port);
        if (ok) return socket;          // it worked!
        net_close(socket);
    }
    if (attempts) { sleep until now + delay (interruptible) }
} while (--attempts);
```
- `attempts = 100`, `delay = SC_TICK_FROM_MS(100)`, so about 10 s in total in forward mode (`sc_server_connect_to` in `server.c`).
- `connect_and_read_byte` = connect, then `net_recv_intr(intr, socket, &byte, 1) != 1` → failure. **The value of the byte is not checked.**
- Only the **first** socket is opened with this retry loop. The audio and control sockets (when enabled) are then opened with a plain `net_socket()` + `net_connect_intr(...)`, with no read and no retry (`server.c`). The server is known to be listening at that point.
- After the connects, `sc_server_connect_to` calls `net_set_tcp_nodelay(control_socket, true)` to disable Nagle on the control socket only. The code comment says it "only impacts the sending side", so it is not set on the other sockets (`server.c` @ v5.0.1). Without it, small control messages (touch, key, the 1-byte `RESET_VIDEO`) can be held back by Nagle/delayed-ACK.
- Order on the client: first socket (video) with retry → audio (if enabled) → control (if enabled) → `TCP_NODELAY` on control → `sc_adb_tunnel_close` (removes the forward) → `device_read_info()` on the first socket.

---

## 6. Device meta (64 bytes)

| Offset | Size | Field |
|---|---|---|
| 0 | 64 | Device name, UTF-8, zero-padded; at most 63 bytes of name, so always NUL-terminated |

- Server: `DEVICE_NAME_FIELD_LENGTH = 64`. The name is truncated at a UTF-8 character boundary with `StringUtils.getUtf8TruncationIndex(bytes, DEVICE_NAME_FIELD_LENGTH - 1)` and written as the full 64-byte buffer to `getFirstSocket()` (`DesktopConnection.java`). It is sent from `Server.java` after `DesktopConnection.open()` returns, only when `send_device_meta=true`.
- `getFirstSocket()` = video if enabled, else audio, else control.
- Client: `net_recv_all_intr(..., buf, 64)`, fails if `r < SC_DEVICE_NAME_FIELD_LENGTH`, then forces `buf[63] = '\0'` (`server.c`, `device_read_info`).
- Rust: `read_exact(&mut [0u8; 64])`, cut at the first NUL, `String::from_utf8_lossy`.

On the video socket the device meta comes **straight after the dummy byte**.

---

## 7. Codec meta (4 bytes) — changed in v4.0

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | u32 BE codec id |

- v5.0.1 `Streamer.writeVideoHeader()` writes **only the 4-byte codec id** (when `send_stream_meta=true`). It is written once, before the encode loop, but only **after** `createMediaCodec()`, `createFormat()` and `capture.init()` have succeeded (`SurfaceEncoder.java` `streamCapture`).
- **There are no width/height fields here any more.** v3.3.4 `Streamer.java` wrote 12 bytes (codec id, width, height). In v4.0+ the dimensions moved to the session packet (section 8).

| Codec | ASCII | u32 |
|---|---|---|
| H.264 | `"h264"` | `0x68323634` |
| H.265 | `"h265"` | `0x68323635` |
| AV1 | `"\0av1"` | `0x00617631` |
| VP8 | `"\0vp8"` | `0x00767038` |
| VP9 | `"\0vp9"` | `0x00767039` |
| (audio) opus / aac / flac / raw | | `0x6F707573` / `0x00616163` / `0x666C6163` / `0x00726177` |

(`develop.md`, plus the comparisons in `demuxer.c`.)

Special values in place of a codec id (`Streamer.writeDisableStream`, handled in `demuxer.c` `run_demuxer`; session-level effect decided in `scrcpy.c` `sc_video_demuxer_on_ended` / `sc_audio_demuxer_on_ended`, all @ v5.0.1):

| Value | Meaning | Official client |
|---|---|---|
| `0x00000000` | Stream explicitly disabled by the device (e.g. audio capture not possible) | Warns, disables that stream's sinks, status `DISABLED`. **Audio:** mirroring goes on without audio (unless `--require-audio`, then `SC_EVENT_DEMUXER_ERROR`). **Video:** `assert(status != SC_DEMUXER_STATUS_DISABLED)` ("The device may not decide to disable the video"); in a release build any non-EOS status pushes `SC_EVENT_DEMUXER_ERROR` and ends the session |
| `0x00000001` | Configuration error on the device | Logs an error, status `ERROR`, ends the session (audio and video) |
| any other unknown value | — | "stream disabled due to unsupported codec": disables the sinks, status stays `ERROR` (its initial value), so `SC_EVENT_DEMUXER_ERROR` ends the session (audio and video) |
| EOF before the 4 bytes | — | "stream disabled due to connection error", status `ERROR`, ends the session |

**0 and 1 are only ever sent on the audio socket.** At v5.0.1, `writeDisableStream()` is called only from `audio/AudioEncoder.java` (lines 209, 271, 275) and `audio/AudioRawRecorder.java` (lines 30, 42); the video path never calls it. On a video configuration failure (`createMediaCodec()` throwing `ConfigurationException` for an unknown/mismatched `video_encoder`, a failure to create the default encoder, or `capture.init()` failing), the video thread ends before `writeVideoHeader()`, reports a fatal termination (`listener.onTerminated(true)` → `Completion.addCompleted(true)` in `Server.java` quits the main looper), and `Server.java`'s `finally` shuts all sockets down. The client sees **EOF where the video codec id should be**.

PhoneBridge rule: on the video socket, treat codec id 0, 1, any unknown id and EOF at the codec id as fatal session errors. Map EOF right after the device meta, together with the server's stderr (e.g. "Video encoder '...' for ... not found", "Could not create default video encoder", "does not match codec type"), to a **video configuration error** rather than a generic connection drop.

---

## 8. Session packet (12 bytes, video only) — new in v4.0

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | u32 BE flags: bit 31 (MSB of byte 0) = 1 (session packet); bit 0 (LSB of byte 3) = `R`, client-resized; other bits 0 |
| 4 | 4 | u32 BE video width (pixels, of the encoded frames) |
| 8 | 4 | u32 BE video height |

```
byte 0   byte 1   byte 2   byte 3
10000000 00000000 00000000 0000000R
```

- Server: `Streamer.writeSessionMeta(width, height, isClientResize)`. It writes the first field as the 32-bit int `(int) (PACKET_FLAG_SESSION >> 32)` = `0x80000000`, `| 1` for a client resize, then width and height as ints (`Streamer.java`). `SurfaceEncoder.java` calls it **once per capture session**, after `mediaCodec.start()`, only if the encoder has not stopped and no reset is pending. `isClientResize` is true only when the reset reasons include `CLIENT_RESIZED` and not `DISPLAY_PROPERTIES_CHANGED` (it is for `new_display`/flex display, which PhoneBridge does not use, so R = 0 in practice).
- Client: `sc_demuxer_is_session(header)` = `header[0] & 0x80`. `sc_demuxer_parse_session` reads width from bytes 4–7, height from 8–11, and `client_resized = header[3] & 1`. The official client rejects width or height 0 ("Invalid session video size") **only on the first session packet**, before the packet loop; session packets inside the loop are parsed and pushed to the sinks with no size check (`demuxer.c` `run_demuxer`). PhoneBridge must check `w != 0 && h != 0` on **every** session packet (a 0×0 size would make WebCodecs `configure()` throw). **The first packet after the codec id must be a session packet**, or the client fails with "Unexpected packet (not a session header)" (`demuxer.c`).
- Audio streams have no session packets (`develop.md`).
- Byte order of width/height: `develop.md` does not say. It is big-endian per `demuxer.c` (`sc_read32be`) and Java `int` writes.
- A session packet has **no payload**. It is 12 bytes in total, the same size as a media header, and you tell them apart by bit 63.

---

## 9. Media packet: 12-byte frame header + payload — flags changed in v4.0

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | u64 BE `pts_and_flags` |
| 8 | 4 | u32 BE `packet_size` (payload length in bytes; must be > 0) |
| 12 | `packet_size` | raw `MediaCodec` output (H.264: Annex-B with start codes) |

`pts_and_flags` bit layout (v4.0+, v5.0.1; server constants in `Streamer.java`, client constants in `demuxer.c`):

| Bit(s) | Constant | Meaning |
|---|---|---|
| 63 | `PACKET_FLAG_SESSION` (`1L << 63`, `Streamer.java` only) | Always 0 for media packets (1 → session packet, section 8). `demuxer.c` has no constant for it; the client tests `header[0] & 0x80` (`sc_demuxer_is_session`). The session header's first field is a u32 `0x80000000 \| R`, not a u64 |
| 62 | `PACKET_FLAG_CONFIG` (`1L << 62`) / `SC_PACKET_FLAG_CONFIG` | Codec config packet (SPS/PPS for H.264, VPS/SPS/PPS for H.265) |
| 61 | `PACKET_FLAG_KEY_FRAME` (`1L << 61`) / `SC_PACKET_FLAG_KEY_FRAME` | Key frame (IDR) |
| 0–60 | `SC_PACKET_PTS_MASK = SC_PACKET_FLAG_KEY_FRAME - 1` | PTS, 61 bits |

v3.3.4 (for contrast, **do not implement**): CONFIG = bit 63, KEY_FRAME = bit 62, 62-bit PTS.

Rules (`Streamer.writeFrameMeta`):
- Config packet: `pts_and_flags` = **exactly `PACKET_FLAG_CONFIG`** (`0x4000_0000_0000_0000`). The PTS field is 0 and has no meaning. The client sets `pts = AV_NOPTS_VALUE` (`demuxer.c`).
- Other packets: `pts_and_flags = pts | (keyFrame ? PACKET_FLAG_KEY_FRAME : 0)`.
- `dts = pts` in the client (`demuxer.c`). There are no B-frames to reorder (UNVERIFIED as a guarantee; scrcpy sets no B-frame options, and its client relies on dts=pts).
- `packet_size == 0` is rejected as an error by the client.
- EOF while reading a header → status EOS, a normal end. EOF in the middle of a packet → error (`demuxer.c`).

### PTS units
**Microseconds.** `Streamer.writePacket(ByteBuffer, MediaCodec.BufferInfo)` passes `bufferInfo.presentationTimeUs` through unchanged. The time base is the encoder's (surface timestamps). It is not wall-clock time and not zero-based. It is a monotonic device clock (UNVERIFIED which clock exactly; treat it as opaque and use only differences). `develop.md` gives no unit; the unit comes from the Java source.

Config handling in the server: for config packets, `Streamer.writePacket` also unwraps OPUS/FLAC headers (audio only). The video config payload is sent as-is.

---

## 10. Merging config packets into the next key frame (official client)

`demuxer.c` creates a `sc_packet_merger` **only when `must_merge_config_packet` is true, which is for H.264 and H.265**. AV1, VP8/VP9 and audio skip it. Every packet goes through `sc_packet_merger_merge()` (`packet_merger.c`):

1. `is_config = packet->pts == AV_NOPTS_VALUE`.
2. On a config packet: free any stored config, `merger->config = malloc(size)`, copy the bytes in, and keep the size, then return true without consuming the packet. Verified: in `run_demuxer` every non-session packet goes `sc_demuxer_recv_packet` → (if `must_merge_config_packet`) `sc_packet_merger_merge` → `sc_packet_source_sinks_push(packet)` unconditionally → `av_packet_unref`. So the official client **also forwards the bare config packet** (pts = `AV_NOPTS_VALUE`) to its sinks (decoder/recorder), and prepends a copy to the next media packet (`demuxer.c`, `packet_merger.c` @ v5.0.1). PhoneBridge deliberately does not forward the bare config packet (section 13, B3).
3. On the next media packet, if a config is stored: `av_grow_packet(packet, config_size)`, then `memmove(packet->data + config_size, packet->data, media_size)`, then `memcpy(packet->data, merger->config, config_size)`. Free the stored config and set it to NULL so it is merged once. The merged packet keeps the media packet's PTS and key flag.
4. If `av_grow_packet` fails: log OOM and return false, keeping the config.

Result: the decoder gets `[SPS][PPS][IDR slice...]` as one access unit. The next packet after a config packet is in practice always the key frame of the new encoder session.

---

## 11. Rotation, size change, RESET_VIDEO and key-frame requests

### 11.1 Capture sessions (`SurfaceEncoder.java`, `CaptureControl.java`)
Before the loop (once): `createMediaCodec()` → `createFormat()` → `capture.init()` → `streamer.writeVideoHeader()` (codec id). The encoder then runs `do { ... } while (alive);`. Each pass (`SurfaceEncoder.java` `streamCapture` @ v5.0.1):
`resetReasons = captureControl.consumeReset()` (break if it contains `RESET_REASON_TERMINATED`; OR in `retainedResetReasons` kept from a failed attempt) → `capture.prepare()` → `size = capture.getSize()` → set `KEY_WIDTH`/`KEY_HEIGHT` → `mediaCodec.configure(format)` → `createInputSurface()` → `capture.start(surface)` → `mediaCodec.start()` → `captureControl.setRunningMediaCodec(mediaCodec)` → **only if** `!stopped && !isResetRequested()`: `writeSessionMeta(w, h, isClientResize)` → `encode()` until EOS. In `finally`: `setRunningMediaCodec(null)`, `capture.stop()` (if started), `mediaCodec.stop()` (if started), `mediaCodec.reset()`, `surface.release()`. The single `MediaCodec` is reset and reused per session; `mediaCodec.release()` and `capture.release()` run **once**, after the loop.

If a reset arrives between `consumeReset()` and the `isResetRequested()` check, that pass writes nothing (no session packet, no config) and the loop goes round again. So several resets in a row can yield a single session packet.

`CaptureControl.reset(reason)` ORs the reason into a bitmask and calls `signalEndOfInputStream()` on the running `MediaCodec`. The current `encode()` then ends with EOS, and the loop starts a **new session**. Reasons:

| Reason | Value | Raised by |
|---|---|---|
| `RESET_REASON_TERMINATED` | 1 | `SurfaceEncoder.stop()`; ends the loop |
| `RESET_REASON_DISPLAY_PROPERTIES_CHANGED` | 2 | `ScreenCapture`'s `DisplayMonitor` callback (rotation, display size change) |
| `RESET_REASON_CLIENT_RESET` | 4 | `Controller.resetVideo()` on `RESET_VIDEO` |
| `RESET_REASON_CLIENT_RESIZED` | 8 | `RESIZE_DISPLAY` (new/flex display only) |

### 11.2 What the client sees on rotation or resize
A new session packet with the new width/height (often swapped), then a **new config packet** (new SPS/PPS, since the codec is reconfigured), then a **key frame**. The client must:
- update the output size from the session packet (never from SPS alone; there may be cropping or alignment),
- drop the old stored config and merge the new one into the next key frame,
- reconfigure the WebCodecs `VideoDecoder` (a new `description`/codec string if the SPS profile or level changed). With Annex-B input and no `description`, a `configure()` with the new coded size, followed by the merged key frame, is enough.

How rotation is detected inside `DisplayMonitor`: UNVERIFIED (not in the files read). `ScreenCapture.java` only shows that the callback calls `reset(RESET_REASON_DISPLAY_PROPERTIES_CHANGED)` and that `prepare()` records the session's display size and rotation.

### 11.3 RESET_VIDEO (key-frame request)
- Control message type **17** = `TYPE_RESET_VIDEO` (`ControlMessage.java` @ v5.0.1; unchanged since v3.3.4).
- **No payload**: the wire message is the single byte `0x11` on the control socket (`ControlMessageReader.java`: `case TYPE_RESET_VIDEO: return ControlMessage.createEmpty(type);`).
- Server: `Controller.resetVideo()` logs "Video capture reset" and calls `surfaceCapture.getCaptureControl().reset(RESET_REASON_CLIENT_RESET)` (`Controller.java`).
- Effect: a **full capture restart**, not just a sync-frame request. The client gets session packet → config packet → key frame. That costs more than `MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME`, but it is the only key-frame mechanism the v5.0.1 protocol exposes. (That scrcpy never uses `PARAMETER_KEY_REQUEST_SYNC_FRAME`: UNVERIFIED, only the files above were read.)
- Rate-limit it (e.g. at most one in flight; ignore further requests until the next key frame arrives).

### 11.4 Periodic key frames
- `SurfaceEncoder.java`: `DEFAULT_I_FRAME_INTERVAL = 10` seconds, `REPEAT_FRAME_DELAY_US = 100_000` (repeat the previous frame after 100 ms of no change), and `KEY_FRAME_RATE` = 60 as a placeholder ("does not affect the actual frame rate").
- With a static screen, frames are produced only on change (`doc/video.md` @ v5.0.1: "a new frame is produced only when the screen content changes"), so IDRs can be sparse.
- `video_codec_options=i-frame-interval:int=N`: the syntax is verified (`scrcpy.1`). The override is verified at the format level: `createFormat` sets `KEY_I_FRAME_INTERVAL = DEFAULT_I_FRAME_INTERVAL` first, and only after all defaults (bit rate, frame rate, color format/range, repeat-previous-frame, priority, latency, `max-fps-to-encoder`) loops over `codecOptions` calling `CodecUtils.setCodecOption(format, key, value)`, which for an `int` does `format.setInteger(key, value)` and overwrites `i-frame-interval` (`SurfaceEncoder.java`, `util/CodecUtils.java` @ v5.0.1). Whether a given hardware encoder honours the value is device-dependent: measure the key-frame spacing.

---

## 12. Cleanup and shutdown

### Server side
- `Server.java`: if `cleanup=true`, `CleanUp.start(options)` runs **before** `DesktopConnection.open`. In the `finally` block: `cleanUp.interrupt()`, stop each async processor (controller, audio, surface encoder), `connection.shutdown()` (shutdownInput/shutdownOutput on every socket), join them, `connection.close()`, then `System.exit(status)` (forced, because the Android SDK may leave non-daemon threads).
- Any processor ending with a fatal error, or the last one finishing, quits the main looper (`Completion.addCompleted`). **Closing the video socket on the client ends the server.** The encoder gets a broken pipe, which "is expected on close because the socket is closed by the client", and does not retry (`SurfaceEncoder.java`).
- `CleanUp.java`: a separate `app_process` that calls `Os.setsid()` so it is not killed along with the server. It reads the server's stdin until it closes, then restores `show_touches`, `stay_on_while_plugged_in`, `screen_off_timeout` and the display power state (and the IME policy for secondary displays). It also deletes the server JAR (section 2).

### Official client side (`server.c`)
- `sc_server_stop`: sets `stopped`, signals, interrupts the intr. `run_server` then calls `net_interrupt` on the video, audio and control sockets.
- It waits up to `WATCHDOG_DELAY = SC_TICK_FROM_SEC(1)` for the `adb shell` process to exit. If it has not, it logs "Killing the server..." and calls `sc_process_terminate(pid)`.
- The forward was already removed after the connect (section 3.3), and again on failure paths.

---

## 13. PhoneBridge implementation notes

### Rust connection sequence (`adb/` + `scrcpy/server.rs` + `scrcpy/video.rs`)
All adb traffic goes through PhoneBridge's own client for the adb server protocol on TCP 5037 (DEVELOPMENT_PLAN Stage 2). PhoneBridge never runs the `adb` CLI per session: a CLI whose version differs from the running server restarts that server, which breaks Android Studio (REVIEW B10, test 0G). The argv forms in sections 2, 3.3 and 4.1 describe what the official client does; the steps below are their 5037 equivalents. Each request is a 4-hex-digit length plus an ASCII service string, answered with `OKAY`, or with `FAIL` plus a length-prefixed message.

1. `sha256(jar) == 764eb6f7…bc536`, otherwise refuse (B15).
2. **Push** (every launch; the server deletes the JAR): connect to 5037 and send `host:transport:<serial>`, then `sync:`. Send `SEND` with `/data/local/tmp/scrcpy-server.jar,33188` (mode 0644), the file as `DATA` chunks of at most 64 KiB, then `DONE` with the mtime. Read `OKAY`, or `FAIL` plus a message, then send `QUIT`. (Sync requests are a 4-byte id plus a u32 little-endian length.)
3. `scid = rand_u32() & 0x7FFF_FFFF`, `name = format!("scrcpy_{scid:08x}")`.
4. **Tunnel.** *Preferred (validate in Stage 1, tests 0A and 0G):* no forward at all. Each socket is its own 5037 connection: `host:transport:<serial>`, then the device service `localabstract:<name>`. After `OKAY` the connection is a raw byte stream to the server's socket, so there are no TCP ports to pick and no forward rules to leak. *Fallback (what the official client does):* send `host-serial:<serial>:forward:norebind:tcp:0;localabstract:<name>`, then read `OKAY`, `OKAY` and the allocated port as a length-prefixed string. Record the port in the session guard **before** going on. Always use `norebind`: a plain `forward:` silently repoints an existing listener, including another session's or the official scrcpy's.
5. **Launch the server.** Query `host-serial:<serial>:features`. If it lists `shell_v2`, open `host:transport:<serial>` and request `shell,v2,raw:CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 5.0.1 scid=<hex> log_level=info tunnel_forward=true audio=false ...`. Shell v2 frames are a 1-byte id plus a u32 little-endian length (1 = stdout, 2 = stderr, 3 = exit code); route them into `tracing`. Otherwise fall back to the legacy `shell:<cmd>` service, whose stdout and stderr arrive merged on one stream. Keep this shell stream open for the whole session.
6. **Video socket**, up to 100 times: open the tunnel (preferred: the `localabstract:` transport connection; fallback: `TcpStream::connect(127.0.0.1:P)`), then `read_exact(&mut [0;1])` → OK → break; otherwise close it and sleep 100 ms. While the server is not listening yet, the preferred path gets `FAIL` (e.g. "closed") where the forward path gets connect-then-EOF: both mean "retry". Abort early on shell-stream EOF or an exit frame (a version mismatch exits fast; show its stderr).
7. **Control socket:** open the tunnel once (no read, no retry). On the TCP fallback, also call `set_nodelay(true)` on it (as `sc_server_connect_to` does).
8. Fallback only: send `host-serial:<serial>:killforward:tcp:<P>` (`OKAY`, `OKAY` on success; the connections stay up).
9. On the video socket: `read_exact` 64 → name; 4 → codec id (0, 1, unknown, or EOF here = fatal; EOF + server stderr = video configuration error, section 7); then the packet loop.

### Packet loop
```rust
let hdr = read_exact::<12>()?;                  // Ok(EOF) at a header boundary = normal end
if hdr[0] & 0x80 != 0 {                         // session packet
    let w = u32::from_be_bytes(hdr[4..8]); let h = u32::from_be_bytes(hdr[8..12]);
    let client_resized = hdr[3] & 1 != 0;
    if w == 0 || h == 0 { return Err(InvalidSessionSize) }  // scrcpy checks only the first one
    emit Session { w, h }; pending_config = None; need_keyframe = true; continue;
}
let pf   = u64::from_be_bytes(hdr[0..8]);
let len  = u32::from_be_bytes(hdr[8..12]);      // reject 0, and cap (e.g. 16 MiB) against corruption
let cfg  = pf & (1 << 62) != 0;
let key  = pf & (1 << 61) != 0;
let pts  = pf & ((1 << 61) - 1);                // microseconds; meaningless if cfg
let data = read_exact_vec(len)?;
if cfg { pending_config = Some(data); continue; }
let data = match pending_config.take() { Some(c) => [c, data].concat(), None => data };
emit Frame { pts_us: pts, key, data };
```

### B3: config packet / SPS-PPS handling
- Implement the merge **in Rust**, so the frontend never sees a config-only chunk: hold the CONFIG packet and prepend it to the next media packet (section 10). This is the H.264/H.265 rule. For AV1, follow scrcpy and do not merge (the AV1 config is the `av1C`-style sequence header; handling it is UNVERIFIED for WebCodecs and out of Stage 0 scope).
- Send the decoder parameters **in-band on the Video Channel**, as a decoder-config record just before the merged key frame, not as a separate event: Tauri orders messages only within one Channel, so a config event on the JSON channel could overtake or trail the frames it describes. Parse the SPS inside the config packet (NAL type 7 after the `00 00 00 01`/`00 00 01` start code) and build `avc1.PPCCLL` from `profile_idc`, `constraint_set` flags and `level_idc` (bytes 1–3 of the SPS NAL payload), in hex. Configure `VideoDecoder` with that string and `codedWidth/codedHeight` from the **session packet**.
- With Annex-B input (start codes, no `description`), Chromium's H.264 decoder takes in-band SPS/PPS on the key frame. That is why the merged chunk works and a bare config chunk does not.
- Reconfigure on every session packet (rotation, RESET_VIDEO, downsize_on_error), since a new config follows each one.
- Fixtures: record a stream that includes at least one rotation, so `fixtures/` covers session → config → key → … → session → config → key.

### B5: dropping frames / key-frame recovery
- Once a delta frame is dropped, drop **every** frame until the next `key` flag. Then send `RESET_VIDEO` (the single byte `0x11` on the control socket) **once** and wait for it.
- Expect `RESET_VIDEO` to produce a new **session packet + config + key frame**, not just an IDR. The decoder-reconfigure path from B3 runs. That is correct but costs tens of ms (UNVERIFIED timing; measure in 0E).
- Do not count on periodic IDRs. The server default is 10 s, and a static screen produces no frames at all. Optionally pass `video_codec_options=i-frame-interval:int=2` (it overrides the default in the `MediaFormat`, section 11.4; whether the device encoder honours it still needs a fixture measuring the key-frame spacing).
- Prefer render-latest-frame plus a bounded decode queue over dropping. Track `pts_us` deltas to spot decoder backlog.

### B9: launch robustness and cleanup
- **Race:** replace `sleep(500ms)` with the connect + read-1-byte loop above (100 × 100 ms, as in scrcpy). EOF/reset on the 1-byte read means "server not listening yet": close and retry. The byte's value does not matter (it is `0x00`). Over the preferred `localabstract:` transport, the not-yet-listening case shows up as `FAIL` on the service request instead; retry the same way.
- **Only the first socket gets the dummy byte.** Do **not** read a byte on the control socket. Doing so would eat the start of a device message, or block forever.
- **Socket order is mandatory:** video → (audio, if `audio=true`) → control. With `audio=true` and no audio socket, the server waits in `accept()` for audio and the control connection is taken as the audio socket. Always pass `audio=false` until audio is built.
- **Tunnels:** with the preferred `localabstract:` transport there is nothing to clean up: closing the connection closes the tunnel. With the forward fallback, remove the forward right after the sockets connect, and also in `Drop` and on error paths, so a crash between steps 4 and 8 does not leak the rule. At start-up, `host:list-forward` (lines of `<serial> <local> <remote>`) shows stale rules. Remove only the ones PhoneBridge recorded itself, because the official scrcpy client uses the same `scrcpy_%08x` names.
- **Server shell stream:** the `shell,v2,raw:` (or legacy `shell:`) connection from step 5 carries the server's log and its exit. Read it continuously into `tracing`; if nobody reads, adbd's buffers fill and the server can block while logging. On stop: close the video and control sockets first (the server exits on broken pipe and `CleanUp` restores the settings), wait up to 1 s (scrcpy's `WATCHDOG_DELAY`), then close the shell stream. Whether closing the shell stream also kills the device-side `app_process` is UNVERIFIED. Closing the sockets is the reliable way to stop the server, and the `setsid` cleanup process runs either way.
- **Version check:** watch the shell stream's stderr (shell v2 id 2, or the merged legacy output) for "does not match the client" and show a clear error. A wrong argv[0] fails before any socket is accepted, so the connect loop would otherwise just time out.
- **Unknown keys are silent:** add a unit test that pins the exact argv string for the default settings (golden test), so a renamed key (e.g. `send_codec_meta`) can't slip in.

---

## 14. UNVERIFIED items (summary)

1. Whether `CleanUp.unlinkSelf()` deletes the JAR on every device and Android version (the code path is unconditional; failure is only logged).
2. Whether a given device's hardware encoder honours a user `i-frame-interval` (the `MediaFormat` override itself is verified, section 11.4).
3. How `DisplayMonitor` detects rotation and size changes internally (`DisplayMonitor.java` not read).
4. That scrcpy never uses `MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME`. `RESET_VIDEO` is the only key-frame mechanism seen.
5. Exactly which clock the PTS is on (the unit, microseconds, is verified). Treat it as an opaque monotonic value.
6. "No B-frames" as a guarantee (the client assumes dts = pts).
7. How long `RESET_VIDEO` takes to produce a key frame (measure in test 0E).
8. Whether closing the server's shell stream (section 13, step 5) also kills the device-side server process.
9. AV1 config handling with WebCodecs (out of Stage 0 scope).
10. Resolved: device → client message layouts are in `protocol/control.md` §3.
