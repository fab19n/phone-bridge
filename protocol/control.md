# scrcpy control socket protocol (pinned: v5.0.1)

Status: reference for `scrcpy/control.rs` (Stage 4). Researched 2026-11-01 against git tag **`v5.0.1`** of
Genymobile/scrcpy. Every fact below cites the file read at that tag. "UNVERIFIED" marks anything not
confirmed from source.

Primary sources (all read at `v5.0.1`, `https://raw.githubusercontent.com/Genymobile/scrcpy/v5.0.1/<path>`):

| Short name | Path |
|---|---|
| `CM.java` | `server/src/main/java/com/genymobile/scrcpy/control/ControlMessage.java` |
| `CMR.java` | `server/src/main/java/com/genymobile/scrcpy/control/ControlMessageReader.java` |
| `Controller.java` | `server/src/main/java/com/genymobile/scrcpy/control/Controller.java` |
| `PositionMapper.java` | `server/src/main/java/com/genymobile/scrcpy/control/PositionMapper.java` |
| `PointersState.java` | `server/src/main/java/com/genymobile/scrcpy/control/PointersState.java` |
| `DM.java` | `server/src/main/java/com/genymobile/scrcpy/control/DeviceMessage.java` |
| `DMW.java` | `server/src/main/java/com/genymobile/scrcpy/control/DeviceMessageWriter.java` |
| `Binary.java` | `server/src/main/java/com/genymobile/scrcpy/util/Binary.java` |
| `control_msg.h/.c` | `app/src/control_msg.h`, `app/src/control_msg.c` |
| `device_msg.h/.c` | `app/src/device_msg.h`, `app/src/device_msg.c` |
| `binary.h` | `app/src/util/binary.h` |
| `input_manager.c` | `app/src/input_manager.c` |
| `mouse_sdk.c` | `app/src/mouse_sdk.c` |
| `input.h`, `keycodes.h` | `app/src/android/input.h`, `app/src/android/keycodes.h` |
| `test_cms.c` | `app/tests/test_control_msg_serialize.c` (upstream golden bytes) |

---

## 1. Transport and framing

- The control socket is the **last** socket opened, after whichever of video/audio are enabled, in connect/accept
  order (`DesktopConnection.open()` handles video if enabled, then audio if enabled, then control). It is the third
  socket only when both video and audio are enabled; with `--no-audio` (or no video) it is the second. The dummy byte
  goes to whichever socket comes first, so control gets it only if video and audio are both disabled
  (`server/src/main/java/com/genymobile/scrcpy/device/DesktopConnection.java` @ v5.0.1).
- The socket is **bidirectional**. Client to device carries control messages. Device to client carries device messages.
- **There is no length prefix or envelope.** Messages are concatenated back to back. Byte 0 of each message is a
  `u8` type, and the rest of the layout depends on the type. The reader parses each one with `DataInputStream`
  (`CMR.java`, `read()`).
- **Endianness: big-endian (network order) for every multi-byte integer, in both directions.** The server uses
  `java.io.DataInputStream` / `DataOutputStream` (`CMR.java`, `DMW.java`). The client uses
  `sc_write16be/32be/64be` and `sc_read16be/32be/64be` (`control_msg.c`, `device_msg.c`).
- Strings are UTF-8 with a length prefix and **no NUL terminator**. The prefix is a byte count, not a character
  count (`CMR.java` `parseString(int sizeBytes)`). Prefix widths are 4 bytes (`parseString()`), 1 byte
  (`parseString(1)`, called "tiny") and 2 bytes for UHID byte arrays (`parseByteArray(2)`).
- Max message size: `MESSAGE_MAX_SIZE = 1 << 18` (256 KiB) in `CMR.java`, and `SC_CONTROL_MSG_MAX_SIZE` in
  `control_msg.h`. In `CMR.java` this constant is private and only used to derive `CLIPBOARD_TEXT_MAX_LENGTH`.
  **The server enforces no size limit while parsing**: `parseByteArray` reads the length prefix and allocates
  `new byte[len]` then `readFully`, with no check against any maximum. The 256 KiB, 262,130 and 300-byte limits are
  client-side only and must be enforced by the Rust serializer; a bogus 4-byte length makes the server block in
  `readFully` or attempt a huge allocation.
- **Unknown type:** `CMR.read()` throws `ControlProtocolException("Unknown event type: ...")`.
  `Controller.handleEvent` returns false on `ControlProtocolException` or `IOException`, and the control loop ends
  (`Controller.java`).
- **Any end of the control thread is fatal to the whole server session.** `Controller.start()` runs `control()` in
  thread `control-recv` inside `try { ... } catch (IOException) { ... } finally { ...; listener.onTerminated(true); }`.
  The finally block runs whatever the cause: unknown type, garbled byte, EOF, or an unchecked exception/Error thrown by
  a handler (e.g. `AssertionError`, `ClassCastException`, see sections 2.17 and 2.18). `Server.java` registers that
  listener as `completion.addCompleted(fatalError)`, and `Completion.addCompleted` calls
  `Looper.getMainLooper().quitSafely()` when `fatalError` is true, which stops every async processor. **Video and
  audio stop too**, not just control (`Controller.java` `start()`, `Server.java` `Completion` @ v5.0.1). The writer
  must therefore never emit a partial or malformed message.

## 2. Client to device messages

### 2.1 Type ids

From `CM.java` (`TYPE_*`), which matches the order of `enum sc_control_msg_type` in `control_msg.h`:

| id | Name | Total size (bytes) | Introduced |
|---:|---|---|---|
| 0 | INJECT_KEYCODE | 14 | |
| 1 | INJECT_TEXT | 5 + n | |
| 2 | INJECT_TOUCH_EVENT | 32 | |
| 3 | INJECT_SCROLL_EVENT | 21 | |
| 4 | BACK_OR_SCREEN_ON | 2 | |
| 5 | EXPAND_NOTIFICATION_PANEL | 1 | |
| 6 | EXPAND_SETTINGS_PANEL | 1 | |
| 7 | COLLAPSE_PANELS | 1 | |
| 8 | GET_CLIPBOARD | 2 | |
| 9 | SET_CLIPBOARD | 14 + n | |
| 10 | SET_DISPLAY_POWER | 2 | |
| 11 | ROTATE_DEVICE | 1 | |
| 12 | UHID_CREATE | 10 + name + desc | |
| 13 | UHID_INPUT | 5 + n | |
| 14 | UHID_DESTROY | 3 | |
| 15 | OPEN_HARD_KEYBOARD_SETTINGS | 1 | |
| 16 | START_APP | 2 + n | |
| 17 | RESET_VIDEO | 1 | |
| 18 | CAMERA_SET_TORCH | 2 | v4.0 |
| 19 | CAMERA_ZOOM_IN | 1 | v4.0 |
| 20 | CAMERA_ZOOM_OUT | 1 | v4.0 |
| 21 | RESIZE_DISPLAY | 5 | v4.0 |
| 22 | SCAN_FILE | 5 + n | v5.0 (present at v5.0.1) |

Other constants in `CM.java` are `SEQUENCE_INVALID = 0`, `COPY_KEY_NONE = 0`, `COPY_KEY_COPY = 1` and `COPY_KEY_CUT = 2`.

The "Introduced" column comes from comparing `ControlMessage.java` at v3.3.4, v4.0 and v5.0.1 in the prior pinning
step. Type 22 (`TYPE_SCAN_FILE`) was introduced in v5.0: it is absent from `ControlMessage.java` @ v4.0 and
present @ v5.0 (`server/src/main/java/com/genymobile/scrcpy/control/ControlMessage.java`).

Sizes come from the return values of `sc_control_msg_serialize` in `control_msg.c` and from the read order in `CMR.java`.

### 2.2 Shared sub-structure: `position` (12 bytes)

`CMR.parsePosition()` and `control_msg.c write_position`:

| Offset | Size | Type | Field |
|---:|---:|---|---|
| +0 | 4 | i32 | x (pixels in the **video frame** coordinate space) |
| +4 | 4 | i32 | y |
| +8 | 2 | u16 | screen_width (width of the video frame the x/y were computed against) |
| +10 | 2 | u16 | screen_height |

"screen" here means the **decoded video frame size**. The upstream client fills it with `im->screen->frame_size`
and converts window coordinates with `sc_screen_convert_window_to_frame_coords` (`input_manager.c`). See section 4 for
why it must match exactly.

### 2.3 INJECT_KEYCODE (0), 14 bytes

`CMR.parseInjectKeycode`:

| Off | Size | Type | Field |
|---:|---:|---|---|
| 0 | 1 | u8 | type = 0x00 |
| 1 | 1 | u8 | action (`AKEY_EVENT_ACTION_DOWN = 0`, `UP = 1`) |
| 2 | 4 | i32 | keycode (Android `AKEYCODE_*`, e.g. HOME = 3, BACK = 4, POWER = 26, VOLUME_UP = 24, VOLUME_DOWN = 25, APP_SWITCH = 187, MENU = 82, ENTER = 66, DEL = 67) |
| 6 | 4 | i32 | repeat count |
| 10 | 4 | i32 | metastate (`AMETA_*`: SHIFT_ON 0x1, ALT_ON 0x2, ALT_LEFT 0x10, ALT_RIGHT 0x20, SHIFT_LEFT 0x40, SHIFT_RIGHT 0x80, CTRL_ON 0x1000, CTRL_LEFT 0x2000, CTRL_RIGHT 0x4000) |

The values come from `input.h` and `keycodes.h`.

POWER caveat (`Controller.injectKeycode` / `scheduleDisplayPowerOff` @ v5.0.1): when `keepDisplayPowerOff` is true
(after SET_DISPLAY_POWER off, section 2.11), an UP of `KEYCODE_POWER` or `KEYCODE_WAKEUP` makes the server schedule
`Device.setDisplayPower(displayId, false)` 200 ms later, so the panel is forced off again. To turn the panel back on in
screen-off mode, send SET_DISPLAY_POWER(1). Upstream golden test (`test_cms.c`): UP, ENTER, repeat 5, SHIFT_ON|SHIFT_LEFT_ON →
`00 01 00000042 00000005 00000041`.

### 2.4 INJECT_TEXT (1), 5 + n bytes

| Off | Size | Type | Field |
|---:|---:|---|---|
| 0 | 1 | u8 | type = 0x01 |
| 1 | 4 | u32 | n = UTF-8 byte length |
| 5 | n | bytes | UTF-8 text |

- Max length: `INJECT_TEXT_MAX_LENGTH = 300` (`CMR.java`) and `SC_CONTROL_MSG_INJECT_TEXT_MAX_LENGTH 300`
  (`control_msg.h`). The **client** truncates to 300 bytes at a UTF-8 boundary (`sc_str_utf8_truncation_index` in
  `control_msg.c`). `CMR.parseInjectText` does not enforce the limit itself. The constant is defined in `CMR.java`, but the
  parse path never checks it. Treat 300 bytes as a hard client-side limit and split longer text into several messages.
- Server handling (`Controller.java`): only when `supportsInputEvents`. Each char goes through
  `KeyComposition.decompose` and then `charMap.getEvents`, and is injected as key events. A char that cannot be mapped is logged
  ("Could not inject char u+...") and skipped. **Non-ASCII text may be silently dropped.** Use SET_CLIPBOARD with
  paste for those characters.

### 2.5 INJECT_TOUCH_EVENT (2), 32 bytes

`CMR.parseInjectTouchEvent` and `control_msg.c`:

| Off | Size | Type | Field |
|---:|---:|---|---|
| 0 | 1 | u8 | type = 0x02 |
| 1 | 1 | u8 | action (`AMOTION_EVENT_ACTION_DOWN 0, UP 1, MOVE 2, CANCEL 3, POINTER_DOWN 5, POINTER_UP 6, HOVER_MOVE 7`) |
| 2 | 8 | u64/i64 | pointer_id |
| 10 | 12 | position | x, y, screen_width, screen_height (section 2.2) |
| 22 | 2 | u16 | pressure, unsigned 16-bit fixed point |
| 24 | 4 | i32 | action_button (`AMOTION_EVENT_BUTTON_*`: PRIMARY 1, SECONDARY 2, TERTIARY 4, BACK 8, FORWARD 16) |
| 28 | 4 | i32 | buttons (bitmask of currently held buttons, same values) |

**Pointer id special values** (`control_msg.h`):

| Constant | Value (u64) | Wire bytes |
|---|---|---|
| `SC_POINTER_ID_MOUSE` | `UINT64_C(-1)` | `FF FF FF FF FF FF FF FF` |
| `SC_POINTER_ID_GENERIC_FINGER` | `UINT64_C(-2)` | `FF FF FF FF FF FF FF FE` |
| `SC_POINTER_ID_VIRTUAL_FINGER` | `UINT64_C(-3)` (extra pointer for pinch-to-zoom) | `FF FF FF FF FF FF FF FD` |
| any other | real finger id (the upstream client passes the SDL `fingerID`) | |

Server side (`Controller.java`): only `POINTER_ID_MOUSE = -1` is declared as a constant. A comment says the
non-mouse branch covers "POINTER_ID_GENERIC_FINGER, POINTER_ID_VIRTUAL_FINGER or real touch from device", so the server only
special-cases −1. Each distinct id gets a slot through `PointersState.getPointerIndex(pointerId)`, with at most
`MAX_POINTERS = 10` (`PointersState.java`). When all slots are full, the server logs "Too many pointers for touch event" and drops the event.

**Tool and source selection** (`Controller.injectTouch`): if `pointerId == POINTER_ID_MOUSE` and either
(`action == HOVER_MOVE`) or secondary buttons are active
(`((actionButton | buttons) & ~BUTTON_PRIMARY) != 0`), the event is injected as `TOOL_TYPE_MOUSE` / `SOURCE_MOUSE`.
On API 23 and above a DOWN may then produce DOWN plus BUTTON_PRESS, and an UP produces BUTTON_RELEASE and then UP only when `buttons == 0`.
**Every other case is injected as `TOOL_TYPE_FINGER` / `SOURCE_TOUCHSCREEN` with `buttons` forced to 0.** When
`pointersState.update()` returns more than one pointer (`pointerCount != 1`, counted before UP pointers are removed),
DOWN becomes `ACTION_POINTER_DOWN` and UP becomes `ACTION_POINTER_UP`, each OR'd with
`(pointerIndex << ACTION_POINTER_INDEX_SHIFT)`, whatever the index, including 0 (e.g. lifting the first finger while a
second is down gives POINTER_UP with index 0) (`Controller.injectTouch` @ v5.0.1).

**Pressure encoding**:
- Client `sc_float_to_u16fp(f)` (`binary.h`): asserts `0.0 <= f <= 1.0`, computes `u = f * 2^16`, and clamps 0x10000
  to `0xFFFF`. So 1.0 → `FF FF`, 0.5 → `80 00` and 0.0 → `00 00`.
- Server `Binary.u16FixedPointToFloat` (`Binary.java`): `0xFFFF → 1.0f`, otherwise `u / 2^16`.
- The upstream client sends 1.0 on DOWN and MOVE and 0.0 on UP for mouse clicks (`mouse_sdk.c`). For real touch it sends the SDL pressure.

**Values the upstream client sends** (`mouse_sdk.c`, `input_manager.c`):
- Mouse click: pointer_id `MOUSE`, or `GENERIC_FINGER` while the virtual finger is down.
  action_button = the changed button, buttons = all held buttons, pressure 1.0 on down and 0.0 on up.
- Mouse motion with a button held: action MOVE, pressure 1.0, buttons = held mask, action_button = 0. With no button held:
  HOVER_MOVE (sent only if hover forwarding is on).
- Finger touch: pointer_id = finger id, action_button = 0, buttons = 0, pressure from the device.

Upstream golden test (`test_cms.c`): DOWN, id 0x1234567887654321, (100,200) on 1080x1920, pressure 1.0,
PRIMARY/PRIMARY →
`02 00 1234567887654321 00000064 000000c8 0438 0780 ffff 00000001 00000001`.

### 2.6 INJECT_SCROLL_EVENT (3), 21 bytes

`CMR.parseInjectScrollEvent`:

| Off | Size | Type | Field |
|---:|---:|---|---|
| 0 | 1 | u8 | type = 0x03 |
| 1 | 12 | position | section 2.2 |
| 13 | 2 | i16 | hscroll, signed 16-bit fixed point of `value / 16` |
| 15 | 2 | i16 | vscroll, same encoding |
| 17 | 4 | i32 | buttons |

**Fixed-point encoding.** The real range is [−16, 16]. The client divides by 16, clamps to [−1, 1] and calls
`sc_float_to_i16fp` (`control_msg.c`). `sc_float_to_i16fp(f)` (`binary.h`) computes `i = f * 2^15` and clamps
0x8000 to `0x7FFF`. The server decodes with `Binary.i16FixedPointToFloat(v) * 16` (`CMR.java`, whose comment says the range
is [−16, 16]). `i16FixedPointToFloat` maps `0x7FFF → 1.0`, otherwise `v / 2^15`. Examples: +16 → `7F FF`,
−16 → `80 00`, +1 → `08 00`, −1 → `F8 00`, 0 → `00 00`.

The values are passed as `AXIS_HSCROLL` / `AXIS_VSCROLL` of an `ACTION_SCROLL` event with `SOURCE_MOUSE`
(`Controller.injectScroll`). The position goes through the same `PositionMapper` check as touch. The upstream client passes the
SDL wheel `x`/`y` unchanged, so one notch = 1.0 (`input_manager.c`). Positive vscroll scrolls up, following the
SDL/Android convention. The direction semantics are UNVERIFIED on device.

Upstream golden test (`test_cms.c`): (260,1026) on 1080x1920, h=16, v=−16, buttons=1 →
`03 00000104 00000402 0438 0780 7fff 8000 00000001`.

### 2.7 BACK_OR_SCREEN_ON (4), 2 bytes

`[0x04][u8 action]`, where action is a key action (DOWN 0 / UP 1) (`CMR.parseBackOrScreenOnEvent`).
Send DOWN and then UP. Server (`Controller.pressBackOrTurnScreenOn` @ v5.0.1): it injects BACK with that action when
- API < 34: `displayId != 0 || Device.isScreenOn(0)` (BACK is always injected when mirroring a non-main display);
- API >= 34: the action display is `DISPLAY_ID_NONE` or `Device.isScreenOn(actionDisplayId)`.

Otherwise (screen off) it ignores non-DOWN actions and presses POWER on DOWN. Gated by `supportsInputEvents`.

### 2.8 EXPAND_NOTIFICATION_PANEL (5), EXPAND_SETTINGS_PANEL (6), COLLAPSE_PANELS (7), 1 byte each

Type byte only (`CMR.read` → `createEmpty`).

### 2.9 GET_CLIPBOARD (8), 2 bytes

`[0x08][u8 copy_key]`, where copy_key is NONE 0 / COPY 1 / CUT 2. Server (`Controller.getClipboard`): if copy_key ≠ NONE
(API 24 and above, input supported), it first injects `KEYCODE_COPY` (278) or `KEYCODE_CUT` (277) and waits for it to finish. Then,
**only when `clipboard_autosync` is off**, it reads the device clipboard and sends a CLIPBOARD device message. With
autosync on (the default), a CLIPBOARD message arrives only if the injected COPY/CUT key (API 24+, input supported)
actually changes the clipboard, through the autosync listener. **GET_CLIPBOARD(NONE) with autosync on produces no
device message at all**; a client must not wait for a reply in that case.

Autosync listener (`Controller` constructor @ v5.0.1): when `clipboardAutosync` is on, it calls
`clipboardManager.addPrimaryClipChangedListener(...)`. The listener returns early while `isSettingClipboard` is set
(changes made by SET_CLIPBOARD), otherwise reads `Device.getClipboardText()` and, if non-null, sends
`DeviceMessage.createClipboard(text)`. Autosync is forced off in camera mode.

### 2.10 SET_CLIPBOARD (9), 14 + n bytes

`CMR.parseSetClipboard`:

| Off | Size | Type | Field |
|---:|---:|---|---|
| 0 | 1 | u8 | type = 0x09 |
| 1 | 8 | u64 | sequence (0 = `SEQUENCE_INVALID` = no ack wanted) |
| 9 | 1 | u8 | paste flag (server: `!= 0` is true; client writes 0/1) |
| 10 | 4 | u32 | n = UTF-8 byte length |
| 14 | n | bytes | text |

- Max length: `CLIPBOARD_TEXT_MAX_LENGTH = MESSAGE_MAX_SIZE − 14 = 262,130` bytes (`CMR.java`,
  `control_msg.h`). The client truncates at a UTF-8 boundary.
- Server (`Controller.setClipboard`): sets the device clipboard (guarded by `isSettingClipboard`, so the change doesn't
  echo back through autosync). If `paste` is set and API is 24 or above and input is supported, it injects `KEYCODE_PASTE` (279). **If
  `sequence != SEQUENCE_INVALID` it always sends ACK_CLIPBOARD(sequence), even if setting the clipboard failed.**
- Upstream MOD+V shortcut (shortcut-modifier branch, `case SDLK_V`, without shift/legacy paste):
  `set_device_clipboard(im, true, SC_SEQUENCE_INVALID)` sends paste=1, seq=0, no ack.
  Plain Ctrl+V with autosync on: `set_device_clipboard(im, false, sequence)` with
  `sequence = im->kp->async_paste ? im->next_sequence : SC_SEQUENCE_INVALID`, then the key is forwarded. A non-zero
  sequence (and thus an ack) is used only when the key processor pastes asynchronously (`input_manager.c` @ v5.0.1).

Upstream golden test (`test_cms.c`): seq 0x0102030405060708, paste, "hello, world!" →
`09 0102030405060708 01 0000000d 68656c6c6f2c20776f726c6421`.

### 2.11 SET_DISPLAY_POWER (10), 2 bytes

`[0x0A][u8 on]`, with 0 = off and 1 = on (`CMR.parseSetDisplayPower`, which uses `readBoolean`, so any non-zero value means on).
Server (`Controller.setDisplayPower` @ v5.0.1): gated by `supportsInputEvents`. It targets `displayId`, or 0 when that
is `DISPLAY_ID_NONE`. On success it sets `keepDisplayPowerOff = (displayId != DISPLAY_ID_NONE && !on)`, so the flag is
cleared on "on" and never set when mirroring a virtual display. If `cleanUp != null` it calls
`cleanUp.setRestoreDisplayPower(!on)`: restore-at-exit is armed when turning off and disarmed when turning on. Mirroring continues with the
physical panel off. This is "screen off" mode, which replaced the old `SET_SCREEN_POWER_MODE` name. The rename history is UNVERIFIED.

### 2.12 ROTATE_DEVICE (11), 1 byte

Type byte only. Server (`Controller.handleEvent` @ v5.0.1): computes `getActionDisplayId()` and, if it is not
`DISPLAY_ID_NONE`, calls `Device.rotateDevice(actionDisplayId)`. Not gated by `supportsInputEvents`. A no-op for
`new_display` before the virtual display exists. Display mode only (in camera mode it throws `AssertionError`, section
2.17). A new video session (session packet with new w/h) is expected to follow; that follow-up is UNVERIFIED on device.

### 2.13 UHID_CREATE (12), UHID_INPUT (13), UHID_DESTROY (14)

`CMR.parseUhidCreate/Input/Destroy` and `control_msg.c`:

UHID_CREATE:

| Off | Size | Type | Field |
|---:|---:|---|---|
| 0 | 1 | u8 | type = 0x0C |
| 1 | 2 | u16 | id (client-chosen HID device id) |
| 3 | 2 | u16 | vendor_id |
| 5 | 2 | u16 | product_id |
| 7 | 1 | u8 | name length L (client truncates to 127) |
| 8 | L | bytes | name (UTF-8) |
| 8+L | 2 | u16 | report descriptor size D |
| 10+L | D | bytes | HID report descriptor |

UHID_INPUT: `[0x0D][u16 id][u16 size][size bytes HID input report]`.
UHID_DESTROY: `[0x0E][u16 id]`.

Server: `UhidManager.open / writeInput / close` (`Controller.java`). Not gated by `supportsInputEvents`. All devices are
closed on exit. Out of scope for PhoneBridge Stage 4.

### 2.14 OPEN_HARD_KEYBOARD_SETTINGS (15), 1 byte

Type byte only.

### 2.15 START_APP (16), 2 + n bytes

`[0x10][u8 n][n bytes UTF-8 name]` (`CMR.parseStartApp`, which uses a 1-byte length, so max 255). Server (`Controller.java`):
a leading `+` means force-stop before starting. After that, a leading `?` means search by app label instead of package
name. Zero or ambiguous matches are logged and nothing happens. Runs on a single-thread executor.

### 2.16 RESET_VIDEO (17), 1 byte

Type byte only. Server: `surfaceCapture.getCaptureControl().reset(RESET_REASON_CLIENT_RESET)`, which restarts
capture. Expect a new session packet plus config packet plus keyframe (`Controller.java`). Handled for both display and camera sources.
Useful for decoder-error recovery (REVIEW B5).

### 2.17 CAMERA_SET_TORCH (18), CAMERA_ZOOM_IN (19), CAMERA_ZOOM_OUT (20)

`[0x12][u8 on]`, `[0x13]`, `[0x14]`. Handled only when the video source is the camera. They are **not** silently
ignored otherwise.

**Source/type rule** (`Controller.handleEvent` @ v5.0.1): RESET_VIDEO (17) is handled first for all sources. After
that, display mode (`!camera`) handles only types 0-16, 21 and 22, and camera mode handles only 18-20. Every other
combination falls through to `throw new AssertionError("Unexpected message type: " + type)`. That Error is not
caught, the control thread ends, and `onTerminated(true)` shuts down the whole server (section 1). So:
- in display mode, **never send types 18-20**;
- in camera mode, send only types 17-20 (any touch, key, clipboard or other message kills the session).

### 2.18 RESIZE_DISPLAY (21), 5 bytes

`[0x15][u16 width][u16 height]` (`CMR.parseResizeDisplay`). The server casts `surfaceCapture` to `NewDisplayCapture`
**without a type check** (`Controller.resizeDisplay` @ v5.0.1). Sent while mirroring the physical display, it throws
`ClassCastException`, which is uncaught (`start()` only catches `IOException`); the control thread ends and
`onTerminated(true)` quits the server main looper, ending the whole session (section 1). **PhoneBridge must never
send it** unless it uses `new_display`.

### 2.19 SCAN_FILE (22), 5 + n bytes

`[0x16][u32 n][n bytes UTF-8 path]` (`CMR.parseScanFile`). The client limit is
`SC_CONTROL_MSG_SCAN_FILE_PATH_MAX_LENGTH 256` (`control_msg.h`). Server: broadcasts
`ACTION_MEDIA_SCANNER_SCAN_FILE` for a `file://` URI, and any throwable is caught and logged (`Controller.java`).

## 3. Device to client messages

Type ids: `DM.java` `TYPE_CLIPBOARD = 0`, `TYPE_ACK_CLIPBOARD = 1`, `TYPE_UHID_OUTPUT = 2`, which matches
`enum sc_device_msg_type` in `device_msg.h`. Max size `DEVICE_MSG_MAX_SIZE = 1 << 18` (`device_msg.h`). Big-endian,
written by `DMW.java`, flushed after each message.

| id | Name | Layout | Size |
|---:|---|---|---|
| 0 | CLIPBOARD | `[0x00][u32 n][n bytes UTF-8]` | 5 + n |
| 1 | ACK_CLIPBOARD | `[0x01][u64 sequence]` | 9 |
| 2 | UHID_OUTPUT | `[0x02][u16 id][u16 size][size bytes]` | 5 + size |

- CLIPBOARD text max: `DMW.CLIPBOARD_TEXT_MAX_LENGTH = MESSAGE_MAX_SIZE − 5 = 262,139` bytes, truncated with
  `StringUtils.getUtf8TruncationIndex` (`DMW.java`). Client constant: `DEVICE_MSG_TEXT_MAX_LENGTH` (`device_msg.h`).
  Verified: `getUtf8TruncationIndex` returns `len` if `len <= max`, otherwise starts at `maxLength` and steps back
  while `utf8[len]` is a continuation byte, so it always cuts on a code-point boundary
  (`server/src/main/java/com/genymobile/scrcpy/util/StringUtils.java` @ v5.0.1).
- Client parsing (`device_msg.c sc_device_msg_deserialize`): returns 0 when the buffer is incomplete (wait for more bytes),
  −1 on an unknown type (fatal), otherwise the number of bytes consumed. A Rust reader should do the same: keep a
  growable buffer and parse in a loop.
- UHID_OUTPUT: the server writes the length with `writeShort` and does not check it (`DMW.java`).
- When CLIPBOARD is sent: on GET_CLIPBOARD with autosync off (section 2.9). With autosync on (the default), it is sent when the device
  clipboard changes, except for changes caused by SET_CLIPBOARD (`PrimaryClipChangedListener` in the `Controller`
  constructor, section 2.9).
- Sending never blocks control processing, but **messages can be lost**: `DeviceMessageSender` puts messages in
  `new ArrayBlockingQueue<>(16)` with `queue.offer(msg)`, drained by thread `control-send`. When the queue is full the
  message is discarded with the warning "Device message dropped" (`server/src/main/java/com/genymobile/scrcpy/control/DeviceMessageSender.java`
  @ v5.0.1). CLIPBOARD and ACK_CLIPBOARD can therefore be silently lost if the client does not read. The client must
  read this direction continuously, and any ack wait must use a timeout.

## 4. Touch with a mismatched screen_width/height

`PositionMapper.map(Position)` (`PositionMapper.java` @ v5.0.1):

```java
Size clientVideoSize = position.getScreenSize();
if (!videoSize.equals(clientVideoSize)) {
    return null;
}
```

- `videoSize` is the size of the **currently encoded video** after `max_size`, crop and orientation. It is not the physical
  screen size. The source comment explains the reason: the client computed the coordinates against a different video size,
  probably because the device rotated after the event was generated.
- **There is no rescaling.** An event whose size is not exactly equal, including a swapped w/h after rotation, is
  **silently dropped**. `Controller.getEventPointAndDisplayId` returns null, logs only at VERBOSE level ("Ignore
  positional event generated for size ..."), and `injectTouch` / `injectScroll` return false (`Controller.java`).
  The control loop continues.
- When the sizes match, the point is transformed by `videoToDeviceMatrix`, a video-pixel to device-pixel mapping.
  `PositionMapper.create` builds it whenever `!videoSize.equals(targetSize) || filterTransform != null`, as
  `ndcToPixels(targetSize) * filter * ndcFromPixels(videoSize)`, so it includes the scaling from video to device
  pixels (e.g. when `max_size` downscales) plus any crop/orientation filter. The point is used as-is only when the
  video size equals the target size and there is no filter. Client coordinates stay in video-frame pixels; the server
  does the scaling.
- When `displayData` is null (no capture mapping), the raw `position.getPoint()` is used directly.
- This applies to every positional message: INJECT_TOUCH_EVENT and INJECT_SCROLL_EVENT.
- Practical consequence for v5.0.1: the size to send is the **width/height from the most recent video session packet**
  (12-byte session packet with bit 63 set, see the video protocol notes). That equals the decoded frame size. A DOWN sent with
  the old size during a rotation is dropped. A dropped event returns at `if (pair == null) return false;` in
  `injectTouch`, before `pointersState.getPointerIndex` is called, so it never touches `PointersState`.
- Orphan behaviour (`PointersState.getPointerIndex/update/cleanUp`, `Controller.injectTouch` @ v5.0.1):
  `getPointerIndex` allocates a new slot for any unknown id, whatever the action. An orphan MOVE is injected as a bare
  `ACTION_MOVE` (no DOWN) and its slot stays occupied until an UP for that id arrives. An orphan UP is injected as
  `ACTION_UP` and its slot is then freed by `cleanUp()`. CANCEL does not free a finger pointer, because the finger
  branch does `setUp(action == ACTION_UP)`.
- Conversely, a **dropped UP** for a pointer whose DOWN was accepted leaves that pointer in `PointersState`
  indefinitely, and Android never sees the UP. If the next gesture reuses the same id (e.g. GENERIC_FINGER −2) the
  stale entry is reused and a fresh `ACTION_DOWN` is injected without the previous UP. If it uses other ids, the stale
  entries count toward `pointerCount`, so a new DOWN becomes `ACTION_POINTER_DOWN`, and after 10 leaked ids every event
  hits "Too many pointers for touch event".

## 5. Golden-byte examples (v5.0.1)

Computed with `struct.pack('>...')` from the layouts above and cross-checked against the upstream `test_cms.c`
vectors (sections 2.3–2.10). Spaces are for readability only.

**Tap DOWN at (100, 200) on a 1080x2400 frame**, pointer_id = GENERIC_FINGER (−2), pressure 1.0, action_button 0,
buttons 0. This is the recommended PhoneBridge form, injected as a finger (32 bytes):

```
02                         type INJECT_TOUCH_EVENT
00                         action DOWN
ff ff ff ff ff ff ff fe    pointer_id = -2 (GENERIC_FINGER)
00 00 00 64                x = 100
00 00 00 c8                y = 200
04 38                      screen_width  = 1080
09 60                      screen_height = 2400
ff ff                      pressure = 1.0
00 00 00 00                action_button = 0
00 00 00 00                buttons = 0
```

The matching UP is `02 01 fffffffffffffffe 00000064 000000c8 0438 0960 0000 00000000 00000000`
(action 1, pressure 0).

The upstream mouse-click form is also accepted: pointer_id `ff ff ff ff ff ff ff ff`, action_button `00000001`, buttons
`00000001`. With PRIMARY only, it is still injected as a finger (section 2.5).

**Key HOME DOWN** (keycode 3, repeat 0, meta 0), 14 bytes:

```
00 00 00 00 00 03 00 00 00 00 00 00 00 00
```

The matching UP is `00 01 00000003 00000000 00000000`.

**INJECT_TEXT "hi"**, 7 bytes:

```
01 00 00 00 02 68 69
```

**SET_CLIPBOARD "abc" with paste**, 17 bytes. Sequence 0 means no ack:

```
09 00 00 00 00 00 00 00 00 01 00 00 00 03 61 62 63
```

With sequence 1 (the server replies with ACK_CLIPBOARD `01 00 00 00 00 00 00 00 01`):

```
09 00 00 00 00 00 00 00 01 01 00 00 00 03 61 62 63
```

## 6. PhoneBridge implementation notes

### B6: ordering (single writer)

- The wire has no framing and no sequence numbers, except SET_CLIPBOARD. Order is whatever order the bytes are written to the
  socket. The server processes messages strictly in that order on one thread (`Controller` loop). Any reordering
  therefore happens on our side.
- Implement `scrcpy/control.rs` as: `enum ControlMsg` → `fn serialize(&self, buf: &mut Vec<u8>)`, plus **one
  writer task** that owns the `TcpStream` write half and drains a `tokio::sync::mpsc::Receiver<ControlMsg>`.
  Tauri commands and the JS control Channel only `try_send`/`send` into the queue. Never write to the socket from a
  command handler.
- Write each message with a single `write_all` of the fully serialized buffer. Never interleave partial writes.
  An unknown or garbled type byte ends the server's control loop and terminates the whole server session, video and
  audio included (section 1).
- Never send a message type that is invalid for the current video source (section 2.17): no types 18-20 for display
  capture, and only 17-20 for camera capture. Doing so throws `AssertionError` and kills the session.
- Coalesce MOVE events in the queue (keep the latest MOVE for a pointer if one is still unsent), but **never drop or reorder
  DOWN/UP**. Upstream `sc_control_msg_is_droppable` returns
  `msg->type != SC_CONTROL_MSG_TYPE_UHID_CREATE && msg->type != SC_CONTROL_MSG_TYPE_UHID_DESTROY`
  (`app/src/control_msg.c` @ v5.0.1), so upstream drops every other message, touch DOWN/UP included, when its queue is
  full. PhoneBridge's stricter rule (drop only MOVE) is a deliberate deviation.
- For test 0F, keep a monotonic `u64` seq in our own queue envelope, not on the wire, and assert order at the
  writer. Use SET_CLIPBOARD's `sequence` (non-zero) and the returned ACK_CLIPBOARD for the only end-to-end ordering
  check the protocol offers. The server's device-message queue holds 16 entries and drops on overflow (section 3),
  so the ACK wait must use a timeout and treat a missing ACK as possible loss, not as a hang.
- Run a separate reader task on the read half to parse device messages (section 3) with the incomplete/consumed/error
  rule of `device_msg.c`.
- Split INJECT_TEXT at 300 bytes on UTF-8 boundaries. Route non-ASCII, emoji, IME and Ctrl+V input through SET_CLIPBOARD with
  paste=1, because the server's charmap injection drops characters it cannot map (section 2.4).
- Send key events as DOWN then UP, and BACK_OR_SCREEN_ON as DOWN then UP.

### B7: coordinate space and required fields

- `screen_width/height` **must equal the current video frame size exactly**, i.e. the w/h from the latest session
  packet (v4.0+). It must not be the device's physical resolution or a metadata size cached at connect time. A mismatch is a silent drop
  (section 4). **JS supplies it:** `coordinates.ts` sends the frame size it mapped (x, y) against, and Rust forwards it
  unchanged. Rust must not stamp the newest size instead: a touch computed against the pre-rotation frame would then pass
  the server's exact-size check and land in the wrong place, rather than being dropped as the server intends.
- `coordinates.ts` must map client → frame pixels as follows: take `getBoundingClientRect()` of the canvas, compute the
  `object-fit: contain` content rect (scale = min(cw/fw, ch/fh), with letterbox offsets), subtract the offset, divide by
  scale, clamp to [0, fw−1] × [0, fh−1] and round. If the canvas backing store is sized in device pixels, account for
  `devicePixelRatio`. Clicks in the letterbox bars should be ignored, not clamped, for DOWN. Clamp only during an active drag.
- Fill every field: action, pointer_id (use `GENERIC_FINGER` −2 for primary-button drag, so the server injects a finger,
  or `MOUSE` −1 with buttons when we later want hover or right-click mouse semantics), pressure (`FF FF` on DOWN/MOVE,
  `00 00` on UP), action_button and buttons (0 for the finger form).
- Rotation race: if a session packet with a new size arrives while a pointer is down, release each active pointer with
  an UP carrying the **new** size, with the last point mapped or clamped into the new frame, so the server really
  injects `ACTION_UP` and frees the `PointersState` slot. Then reset local pointer state. Do not rely on an old-size UP
  (it is dropped and leaks the server-side pointer) or on CANCEL (it does not free a finger pointer) (section 4).
- Scroll: send `INJECT_SCROLL_EVENT` with the same position rules. Convert browser `WheelEvent.deltaY` to notches
  (e.g. −deltaY/100 for pixel mode, 1 per line in line mode), clamp to [−16, 16] and encode like upstream
  `sc_float_to_i16fp`: clamp `v/16` to [−1, 1], multiply by 32768 and **truncate toward zero** (`as i32` in Rust),
  then clamp 0x8000 to 0x7FFF (`int32_t i = f * 0x1p15f` in `app/src/util/binary.h` @ v5.0.1). Do not round, or
  golden bytes will differ from upstream for some fractional inputs. Invert the sign according to the SDL convention (positive vscroll = scroll content up). Verify the direction on
  device (UNVERIFIED).
- Never send RESIZE_DISPLAY (21) while mirroring the main display (section 2.18).

## 7. UNVERIFIED items (summary)

1. That a new session packet follows ROTATE_DEVICE (the handler itself is documented in section 2.12).
2. Scroll direction semantics on device (positive vscroll = up) and the browser-delta mapping.
3. History of the SET_DISPLAY_POWER name (formerly `SET_SCREEN_POWER_MODE`).

Items resolved from source @ v5.0.1 and removed: SCAN_FILE introduction (v5.0), ROTATE_DEVICE handler, autosync
listener, `getUtf8TruncationIndex` boundary, `DeviceMessageSender` queue behaviour, `sc_control_msg_is_droppable`
rule, orphan MOVE/UP behaviour, RESIZE_DISPLAY crash consequence.
