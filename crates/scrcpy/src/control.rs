//! The control socket (`protocol/control.md`): client → device messages and device → client messages.
//! There is no framing: each message is a type byte followed by its fields, and every integer is
//! big-endian (§1). The server ends the whole session on a message it can't parse, so write each
//! [`ControlMessage`] with a single `write_all` from one writer task (§6, B6).

// Client → device type ids (`ControlMessage.java` `TYPE_*` @ v5.0.1, control.md §2.1).
pub const TYPE_INJECT_KEYCODE: u8 = 0;
pub const TYPE_INJECT_TEXT: u8 = 1;
pub const TYPE_INJECT_TOUCH_EVENT: u8 = 2;
pub const TYPE_INJECT_SCROLL_EVENT: u8 = 3;
pub const TYPE_BACK_OR_SCREEN_ON: u8 = 4;
pub const TYPE_EXPAND_NOTIFICATION_PANEL: u8 = 5;
pub const TYPE_EXPAND_SETTINGS_PANEL: u8 = 6;
pub const TYPE_COLLAPSE_PANELS: u8 = 7;
pub const TYPE_GET_CLIPBOARD: u8 = 8;
pub const TYPE_SET_CLIPBOARD: u8 = 9;
pub const TYPE_SET_DISPLAY_POWER: u8 = 10;
pub const TYPE_ROTATE_DEVICE: u8 = 11;
/// Not implemented here, like the other UHID types (control.md §2.13).
pub const TYPE_UHID_CREATE: u8 = 12;
pub const TYPE_UHID_INPUT: u8 = 13;
pub const TYPE_UHID_DESTROY: u8 = 14;
pub const TYPE_OPEN_HARD_KEYBOARD_SETTINGS: u8 = 15;
pub const TYPE_START_APP: u8 = 16;
pub const TYPE_RESET_VIDEO: u8 = 17;
/// Camera source only, like types 19 and 20; not implemented here. **Never send a camera type while
/// mirroring a display**: the server throws `AssertionError`, which ends its control thread and the
/// whole session (control.md §2.17).
pub const TYPE_CAMERA_SET_TORCH: u8 = 18;
/// Camera source only: never send while mirroring a display (see [`TYPE_CAMERA_SET_TORCH`]).
pub const TYPE_CAMERA_ZOOM_IN: u8 = 19;
/// Camera source only: never send while mirroring a display (see [`TYPE_CAMERA_SET_TORCH`]).
pub const TYPE_CAMERA_ZOOM_OUT: u8 = 20;
/// `new_display` only; not implemented here. On the main display the server's unchecked cast ends
/// the session (control.md §2.18).
pub const TYPE_RESIZE_DISPLAY: u8 = 21;
/// Not implemented here (control.md §2.19).
pub const TYPE_SCAN_FILE: u8 = 22;

// Device → client type ids (`DeviceMessage.java` `TYPE_*` @ v5.0.1, control.md §3).
pub const DEVICE_MSG_TYPE_CLIPBOARD: u8 = 0;
pub const DEVICE_MSG_TYPE_ACK_CLIPBOARD: u8 = 1;
pub const DEVICE_MSG_TYPE_UHID_OUTPUT: u8 = 2;

/// Largest message in either direction (`MESSAGE_MAX_SIZE`, `SC_CONTROL_MSG_MAX_SIZE`): 256 KiB.
pub const MESSAGE_MAX_SIZE: usize = 1 << 18;
/// INJECT_TEXT limit in UTF-8 bytes (`INJECT_TEXT_MAX_LENGTH`).
pub const INJECT_TEXT_MAX_LENGTH: usize = 300;
/// SET_CLIPBOARD text limit in UTF-8 bytes, so the message fits [`MESSAGE_MAX_SIZE`].
pub const CLIPBOARD_TEXT_MAX_LENGTH: usize = MESSAGE_MAX_SIZE - 14;
/// CLIPBOARD device message limit in UTF-8 bytes (`DeviceMessageWriter.CLIPBOARD_TEXT_MAX_LENGTH`).
pub const DEVICE_CLIPBOARD_TEXT_MAX_LENGTH: usize = MESSAGE_MAX_SIZE - 5;

/// SET_CLIPBOARD `sequence` that asks for no ACK_CLIPBOARD (`SEQUENCE_INVALID`).
pub const SEQUENCE_INVALID: u64 = 0;

/// `SC_POINTER_ID_MOUSE` (−1). The server injects a mouse only for hover or non-primary buttons,
/// otherwise a finger (control.md §2.5).
pub const POINTER_ID_MOUSE: u64 = u64::MAX;
/// `SC_POINTER_ID_GENERIC_FINGER` (−2), PhoneBridge's id for a primary-button drag (control.md B7).
pub const POINTER_ID_GENERIC_FINGER: u64 = u64::MAX - 1;
/// `SC_POINTER_ID_VIRTUAL_FINGER` (−3), the extra pointer for pinch-to-zoom.
pub const POINTER_ID_VIRTUAL_FINGER: u64 = u64::MAX - 2;

// `AMOTION_EVENT_BUTTON_*` for `action_button` and `buttons` (`app/src/android/input.h`).
pub const BUTTON_PRIMARY: u32 = 1;
pub const BUTTON_SECONDARY: u32 = 2;
pub const BUTTON_TERTIARY: u32 = 4;
pub const BUTTON_BACK: u32 = 8;
pub const BUTTON_FORWARD: u32 = 16;

// `AKEYCODE_*` (`app/src/android/keycodes.h`).
pub const KEYCODE_HOME: u32 = 3;
pub const KEYCODE_BACK: u32 = 4;
pub const KEYCODE_VOLUME_UP: u32 = 24;
pub const KEYCODE_VOLUME_DOWN: u32 = 25;
pub const KEYCODE_POWER: u32 = 26;
pub const KEYCODE_ENTER: u32 = 66;
pub const KEYCODE_DEL: u32 = 67;
pub const KEYCODE_MENU: u32 = 82;
pub const KEYCODE_APP_SWITCH: u32 = 187;

// `AMETA_*` for `metastate` (`app/src/android/input.h`).
pub const META_SHIFT_ON: u32 = 0x1;
pub const META_ALT_ON: u32 = 0x2;
pub const META_ALT_LEFT_ON: u32 = 0x10;
pub const META_ALT_RIGHT_ON: u32 = 0x20;
pub const META_SHIFT_LEFT_ON: u32 = 0x40;
pub const META_SHIFT_RIGHT_ON: u32 = 0x80;
pub const META_CTRL_ON: u32 = 0x1000;
pub const META_CTRL_LEFT_ON: u32 = 0x2000;
pub const META_CTRL_RIGHT_ON: u32 = 0x4000;

/// `AKEY_EVENT_ACTION_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyAction {
    Down = 0,
    Up = 1,
}

/// The `AMOTION_EVENT_ACTION_*` values a client sends (control.md §2.5). There is no POINTER_DOWN
/// or POINTER_UP: send DOWN and UP and the server adds the pointer index itself, and only UP frees
/// its slot for a finger, so end a gesture with UP rather than CANCEL (`Controller.injectTouch`
/// @ v5.0.1, control.md §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MotionAction {
    Down = 0,
    Up = 1,
    Move = 2,
    Cancel = 3,
    HoverMove = 7,
}

/// The key GET_CLIPBOARD injects before reading the clipboard (`COPY_KEY_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CopyKey {
    None = 0,
    Copy = 1,
    Cut = 2,
}

/// A point in video-frame pixels, with the frame size it was computed against (control.md §2.2).
/// The server drops the event unless that size equals the current video size exactly (§4), so pass
/// the size the frontend mapped against, not the newest one (B7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub screen_width: u16,
    pub screen_height: u16,
}

/// A client → device message (control.md §2), for a display video source.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlMessage {
    /// 14 bytes. `metastate` is a mask of `META_*`.
    InjectKeycode {
        action: KeyAction,
        keycode: u32,
        repeat: u32,
        metastate: u32,
    },
    /// 5 + n bytes. `text` is truncated to [`INJECT_TEXT_MAX_LENGTH`] bytes on a character
    /// boundary, as the upstream client does; use [`text_chunks`] to send all of a longer text. The
    /// server drops characters its key map can't type, so send other text with
    /// `SetClipboard { paste: true }` (§2.4).
    InjectText {
        text: String,
    },
    /// 32 bytes. `pressure` (0.0 to 1.0, clamped) is sent as unsigned 16-bit fixed point.
    InjectTouch {
        action: MotionAction,
        /// A finger id or one of the `POINTER_ID_*` values.
        pointer_id: u64,
        position: Position,
        pressure: f32,
        action_button: u32,
        buttons: u32,
    },
    /// 21 bytes. `hscroll`/`vscroll` are in wheel notches, clamped to −16..=16 and sent as signed
    /// 16-bit fixed point of `value / 16`.
    InjectScroll {
        position: Position,
        hscroll: f32,
        vscroll: f32,
        buttons: u32,
    },
    /// BACK, or POWER on DOWN while the screen is off. Send DOWN then UP.
    BackOrScreenOn {
        action: KeyAction,
    },
    ExpandNotificationPanel,
    ExpandSettingsPanel,
    CollapsePanels,
    /// With clipboard autosync on (the server default) only a COPY/CUT that changes the clipboard
    /// produces a CLIPBOARD reply, so never wait for one (§2.9).
    GetClipboard {
        copy_key: CopyKey,
    },
    /// 14 + n bytes. A `sequence` other than [`SEQUENCE_INVALID`] asks for an ACK_CLIPBOARD.
    /// `text` is truncated to [`CLIPBOARD_TEXT_MAX_LENGTH`] bytes on a character boundary, as
    /// upstream does.
    SetClipboard {
        sequence: u64,
        paste: bool,
        text: String,
    },
    /// Turns the device's panel off or on; mirroring continues.
    SetDisplayPower {
        on: bool,
    },
    RotateDevice,
    /// Restarts the capture: expect a session packet, a config packet and a key frame (§2.16).
    ResetVideo,
}

impl ControlMessage {
    /// The type byte (`TYPE_*`).
    pub fn type_id(&self) -> u8 {
        match self {
            Self::InjectKeycode { .. } => TYPE_INJECT_KEYCODE,
            Self::InjectText { .. } => TYPE_INJECT_TEXT,
            Self::InjectTouch { .. } => TYPE_INJECT_TOUCH_EVENT,
            Self::InjectScroll { .. } => TYPE_INJECT_SCROLL_EVENT,
            Self::BackOrScreenOn { .. } => TYPE_BACK_OR_SCREEN_ON,
            Self::ExpandNotificationPanel => TYPE_EXPAND_NOTIFICATION_PANEL,
            Self::ExpandSettingsPanel => TYPE_EXPAND_SETTINGS_PANEL,
            Self::CollapsePanels => TYPE_COLLAPSE_PANELS,
            Self::GetClipboard { .. } => TYPE_GET_CLIPBOARD,
            Self::SetClipboard { .. } => TYPE_SET_CLIPBOARD,
            Self::SetDisplayPower { .. } => TYPE_SET_DISPLAY_POWER,
            Self::RotateDevice => TYPE_ROTATE_DEVICE,
            Self::ResetVideo => TYPE_RESET_VIDEO,
        }
    }

    /// Appends the message's wire bytes to `buf` (`control_msg.c` `sc_control_msg_serialize`). It
    /// cannot fail: texts are truncated and pressure and scroll values clamped, as documented on
    /// each variant.
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        buf.push(self.type_id());
        match self {
            Self::InjectKeycode {
                action,
                keycode,
                repeat,
                metastate,
            } => {
                buf.push(*action as u8);
                buf.extend_from_slice(&keycode.to_be_bytes());
                buf.extend_from_slice(&repeat.to_be_bytes());
                buf.extend_from_slice(&metastate.to_be_bytes());
            }
            Self::InjectText { text } => put_string(buf, text, INJECT_TEXT_MAX_LENGTH),
            Self::InjectTouch {
                action,
                pointer_id,
                position,
                pressure,
                action_button,
                buttons,
            } => {
                buf.push(*action as u8);
                buf.extend_from_slice(&pointer_id.to_be_bytes());
                put_position(buf, position);
                buf.extend_from_slice(&u16_fixed_point(*pressure).to_be_bytes());
                buf.extend_from_slice(&action_button.to_be_bytes());
                buf.extend_from_slice(&buttons.to_be_bytes());
            }
            Self::InjectScroll {
                position,
                hscroll,
                vscroll,
                buttons,
            } => {
                put_position(buf, position);
                buf.extend_from_slice(&scroll_fixed_point(*hscroll).to_be_bytes());
                buf.extend_from_slice(&scroll_fixed_point(*vscroll).to_be_bytes());
                buf.extend_from_slice(&buttons.to_be_bytes());
            }
            Self::BackOrScreenOn { action } => buf.push(*action as u8),
            Self::GetClipboard { copy_key } => buf.push(*copy_key as u8),
            Self::SetClipboard {
                sequence,
                paste,
                text,
            } => {
                buf.extend_from_slice(&sequence.to_be_bytes());
                buf.push(u8::from(*paste));
                put_string(buf, text, CLIPBOARD_TEXT_MAX_LENGTH);
            }
            Self::SetDisplayPower { on } => buf.push(u8::from(*on)),
            Self::ExpandNotificationPanel
            | Self::ExpandSettingsPanel
            | Self::CollapsePanels
            | Self::RotateDevice
            | Self::ResetVideo => {}
        }
    }

    /// The message's wire bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.serialize(&mut buf);
        buf
    }
}

/// Splits `text` into pieces of at most [`INJECT_TEXT_MAX_LENGTH`] bytes on character boundaries,
/// one INJECT_TEXT each (control.md §6). Empty text has no pieces.
pub fn text_chunks(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let (chunk, tail) = rest.split_at(rest.floor_char_boundary(INJECT_TEXT_MAX_LENGTH));
        rest = tail;
        Some(chunk)
    })
}

/// A u32 length and the UTF-8 bytes, truncated to `max_len` bytes on a character boundary
/// (`control_msg.c` `write_string`).
fn put_string(buf: &mut Vec<u8>, text: &str, max_len: usize) {
    let text = &text[..text.floor_char_boundary(max_len)];
    // `max_len` is at most CLIPBOARD_TEXT_MAX_LENGTH, so the length fits.
    buf.extend_from_slice(&(text.len() as u32).to_be_bytes());
    buf.extend_from_slice(text.as_bytes());
}

fn put_position(buf: &mut Vec<u8>, position: &Position) {
    buf.extend_from_slice(&position.x.to_be_bytes());
    buf.extend_from_slice(&position.y.to_be_bytes());
    buf.extend_from_slice(&position.screen_width.to_be_bytes());
    buf.extend_from_slice(&position.screen_height.to_be_bytes());
}

/// `sc_float_to_u16fp` (`app/src/util/binary.h`): 0.0..=1.0 → `value * 2^16`, truncated, with 1.0 →
/// `0xFFFF`. NaN and out-of-range values are clamped first.
fn u16_fixed_point(value: f32) -> u16 {
    let value = if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    };
    ((value * 65536.0) as u32).min(0xFFFF) as u16
}

/// `sc_float_to_i16fp(clamp(notches / 16, -1, 1))` (`control_msg.c`): `value * 2^15` truncated
/// toward zero, with +16 → `0x7FFF` (control.md §2.6, B7). NaN is sent as 0.
fn scroll_fixed_point(notches: f32) -> i16 {
    if notches.is_nan() {
        return 0;
    }
    let value = (notches / 16.0).clamp(-1.0, 1.0);
    ((value * 32768.0) as i32).min(0x7FFF) as i16
}

/// A device → client message (control.md §3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceMessage {
    /// The device clipboard, on GET_CLIPBOARD with autosync off or when it changes with autosync
    /// on. Invalid UTF-8 is replaced.
    Clipboard { text: String },
    /// The `sequence` of a SET_CLIPBOARD. The server drops device messages when its 16-entry queue
    /// is full, so wait for an ack with a timeout.
    AckClipboard { sequence: u64 },
    /// An output report for a UHID device.
    UhidOutput { id: u16, data: Vec<u8> },
}

/// A device → client stream that can't be parsed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ControlError {
    /// The stream can't be resynchronised after an unknown type, so this ends the session
    /// (`device_msg.c` returns -1).
    #[error("unknown device message type {0}")]
    UnknownDeviceMessage(u8),
    /// The server never sends more than [`DEVICE_CLIPBOARD_TEXT_MAX_LENGTH`] bytes; a larger length
    /// means a corrupt stream.
    #[error("device clipboard length {0} exceeds the protocol maximum")]
    ClipboardTooLong(u32),
}

/// Parses the device message at the start of `buf`: `Ok(None)` while it is incomplete, otherwise
/// the message and the number of bytes it used (`device_msg.c` `sc_device_msg_deserialize`). Keep
/// unparsed bytes and call again as more arrive; one message never needs more than
/// [`MESSAGE_MAX_SIZE`] bytes.
pub fn parse_device_message(buf: &[u8]) -> Result<Option<(DeviceMessage, usize)>, ControlError> {
    let Some((&kind, body)) = buf.split_first() else {
        return Ok(None);
    };
    match kind {
        DEVICE_MSG_TYPE_CLIPBOARD => {
            let Some(&len) = body.first_chunk::<4>() else {
                return Ok(None);
            };
            let len = u32::from_be_bytes(len);
            let Some(size) = usize::try_from(len)
                .ok()
                .filter(|&size| size <= DEVICE_CLIPBOARD_TEXT_MAX_LENGTH)
            else {
                return Err(ControlError::ClipboardTooLong(len));
            };
            let Some(text) = body.get(4..4 + size) else {
                return Ok(None);
            };
            let text = String::from_utf8_lossy(text).into_owned();
            Ok(Some((DeviceMessage::Clipboard { text }, 5 + size)))
        }
        DEVICE_MSG_TYPE_ACK_CLIPBOARD => {
            let Some(&sequence) = body.first_chunk::<8>() else {
                return Ok(None);
            };
            let sequence = u64::from_be_bytes(sequence);
            Ok(Some((DeviceMessage::AckClipboard { sequence }, 9)))
        }
        DEVICE_MSG_TYPE_UHID_OUTPUT => {
            let Some(&[id_hi, id_lo, size_hi, size_lo]) = body.first_chunk::<4>() else {
                return Ok(None);
            };
            let size = usize::from(u16::from_be_bytes([size_hi, size_lo]));
            let Some(data) = body.get(4..4 + size) else {
                return Ok(None);
            };
            let id = u16::from_be_bytes([id_hi, id_lo]);
            Ok(Some((
                DeviceMessage::UhidOutput {
                    id,
                    data: data.to_vec(),
                },
                5 + size,
            )))
        }
        other => Err(ControlError::UnknownDeviceMessage(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes from hex digits, ignoring whitespace (the notation of control.md §5).
    fn hex(digits: &str) -> Vec<u8> {
        let digits: Vec<u8> = digits
            .bytes()
            .filter(|b| !b.is_ascii_whitespace())
            .collect();
        digits
            .chunks(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    fn touch(
        action: MotionAction,
        pointer_id: u64,
        pressure: f32,
        buttons: u32,
        size: (u16, u16),
    ) -> ControlMessage {
        ControlMessage::InjectTouch {
            action,
            pointer_id,
            position: Position {
                x: 100,
                y: 200,
                screen_width: size.0,
                screen_height: size.1,
            },
            pressure,
            action_button: buttons,
            buttons,
        }
    }

    fn key(action: KeyAction, keycode: u32) -> ControlMessage {
        ControlMessage::InjectKeycode {
            action,
            keycode,
            repeat: 0,
            metastate: 0,
        }
    }

    fn scroll(hscroll: f32, vscroll: f32) -> ControlMessage {
        ControlMessage::InjectScroll {
            position: Position {
                x: 260,
                y: 1026,
                screen_width: 1080,
                screen_height: 1920,
            },
            hscroll,
            vscroll,
            buttons: 0,
        }
    }

    fn text(text: &str) -> ControlMessage {
        ControlMessage::InjectText { text: text.into() }
    }

    fn set_clipboard(sequence: u64, paste: bool, text: &str) -> ControlMessage {
        ControlMessage::SetClipboard {
            sequence,
            paste,
            text: text.into(),
        }
    }

    /// The pressure field of a touch message.
    fn pressure_bytes(pressure: f32) -> [u8; 2] {
        let bytes = touch(
            MotionAction::Down,
            POINTER_ID_GENERIC_FINGER,
            pressure,
            0,
            (1080, 2400),
        )
        .to_bytes();
        [bytes[22], bytes[23]]
    }

    /// The hscroll field of a scroll message.
    fn scroll_bytes(notches: f32) -> [u8; 2] {
        let bytes = scroll(notches, 0.0).to_bytes();
        [bytes[13], bytes[14]]
    }

    // Golden bytes from control.md §5.

    #[test]
    fn golden_tap_down_and_up_on_1080x2400() {
        let down = touch(
            MotionAction::Down,
            POINTER_ID_GENERIC_FINGER,
            1.0,
            0,
            (1080, 2400),
        );
        assert_eq!(
            down.to_bytes(),
            hex("02 00 fffffffffffffffe 00000064 000000c8 0438 0960 ffff 00000000 00000000")
        );
        let up = touch(
            MotionAction::Up,
            POINTER_ID_GENERIC_FINGER,
            0.0,
            0,
            (1080, 2400),
        );
        assert_eq!(
            up.to_bytes(),
            hex("02 01 fffffffffffffffe 00000064 000000c8 0438 0960 0000 00000000 00000000")
        );
    }

    #[test]
    fn golden_mouse_click_form() {
        let down = touch(
            MotionAction::Down,
            POINTER_ID_MOUSE,
            1.0,
            BUTTON_PRIMARY,
            (1080, 2400),
        );
        assert_eq!(
            down.to_bytes(),
            hex("02 00 ffffffffffffffff 00000064 000000c8 0438 0960 ffff 00000001 00000001")
        );
    }

    #[test]
    fn golden_key_home_down_and_up() {
        assert_eq!(
            key(KeyAction::Down, KEYCODE_HOME).to_bytes(),
            hex("00 00 00 00 00 03 00 00 00 00 00 00 00 00")
        );
        assert_eq!(
            key(KeyAction::Up, KEYCODE_HOME).to_bytes(),
            hex("00 01 00000003 00000000 00000000")
        );
    }

    #[test]
    fn golden_inject_text_hi() {
        assert_eq!(text("hi").to_bytes(), hex("01 00 00 00 02 68 69"));
    }

    #[test]
    fn golden_set_clipboard_abc_with_paste() {
        assert_eq!(
            set_clipboard(SEQUENCE_INVALID, true, "abc").to_bytes(),
            hex("09 00 00 00 00 00 00 00 00 01 00 00 00 03 61 62 63")
        );
        assert_eq!(
            set_clipboard(1, true, "abc").to_bytes(),
            hex("09 00 00 00 00 00 00 00 01 01 00 00 00 03 61 62 63")
        );
    }

    #[test]
    fn golden_ack_clipboard_reply() {
        assert_eq!(
            parse_device_message(&hex("01 00 00 00 00 00 00 00 01")),
            Ok(Some((DeviceMessage::AckClipboard { sequence: 1 }, 9)))
        );
    }

    // One test per implemented type, with the upstream vectors where there is one
    // (`app/tests/test_control_msg_serialize.c` @ v5.0.1).

    #[test]
    fn serializes_inject_keycode() {
        let msg = ControlMessage::InjectKeycode {
            action: KeyAction::Up,
            keycode: KEYCODE_ENTER,
            repeat: 5,
            metastate: META_SHIFT_ON | META_SHIFT_LEFT_ON,
        };
        assert_eq!(msg.to_bytes(), hex("00 01 00000042 00000005 00000041"));
    }

    #[test]
    fn serializes_inject_text() {
        assert_eq!(
            text("hello, world!").to_bytes(),
            hex("01 0000000d 68656c6c6f2c20776f726c6421")
        );
        assert_eq!(text("").to_bytes(), hex("01 00000000"));
        assert_eq!(
            text("é").to_bytes(),
            hex("01 00000002 c3a9"),
            "the length counts bytes"
        );
    }

    #[test]
    fn serializes_inject_touch_event() {
        let msg = ControlMessage::InjectTouch {
            action: MotionAction::Down,
            pointer_id: 0x1234_5678_8765_4321,
            position: Position {
                x: 100,
                y: 200,
                screen_width: 1080,
                screen_height: 1920,
            },
            pressure: 1.0,
            action_button: BUTTON_PRIMARY,
            buttons: BUTTON_PRIMARY,
        };
        assert_eq!(
            msg.to_bytes(),
            hex("02 00 1234567887654321 00000064 000000c8 0438 0780 ffff 00000001 00000001")
        );
        let pinch = touch(
            MotionAction::Move,
            POINTER_ID_VIRTUAL_FINGER,
            1.0,
            0,
            (1080, 2400),
        );
        assert_eq!(&pinch.to_bytes()[..10], hex("02 02 fffffffffffffffd"));
        let negative = ControlMessage::InjectTouch {
            action: MotionAction::HoverMove,
            pointer_id: POINTER_ID_MOUSE,
            position: Position {
                x: -1,
                y: -2,
                screen_width: 1080,
                screen_height: 2400,
            },
            pressure: 0.0,
            action_button: 0,
            buttons: BUTTON_SECONDARY | BUTTON_TERTIARY,
        };
        assert_eq!(
            negative.to_bytes(),
            hex("02 07 ffffffffffffffff ffffffff fffffffe 0438 0960 0000 00000000 00000006")
        );
    }

    #[test]
    fn serializes_inject_scroll_event() {
        let msg = ControlMessage::InjectScroll {
            position: Position {
                x: 260,
                y: 1026,
                screen_width: 1080,
                screen_height: 1920,
            },
            hscroll: 16.0,
            vscroll: -16.0,
            buttons: 1,
        };
        assert_eq!(
            msg.to_bytes(),
            hex("03 00000104 00000402 0438 0780 7fff 8000 00000001")
        );
    }

    #[test]
    fn serializes_back_or_screen_on() {
        let up = ControlMessage::BackOrScreenOn {
            action: KeyAction::Up,
        };
        assert_eq!(up.to_bytes(), hex("04 01"));
        let down = ControlMessage::BackOrScreenOn {
            action: KeyAction::Down,
        };
        assert_eq!(down.to_bytes(), hex("04 00"));
    }

    #[test]
    fn serializes_expand_notification_panel() {
        assert_eq!(ControlMessage::ExpandNotificationPanel.to_bytes(), [0x05]);
    }

    #[test]
    fn serializes_expand_settings_panel() {
        assert_eq!(ControlMessage::ExpandSettingsPanel.to_bytes(), [0x06]);
    }

    #[test]
    fn serializes_collapse_panels() {
        assert_eq!(ControlMessage::CollapsePanels.to_bytes(), [0x07]);
    }

    #[test]
    fn serializes_get_clipboard() {
        for (copy_key, byte) in [(CopyKey::None, 0), (CopyKey::Copy, 1), (CopyKey::Cut, 2)] {
            assert_eq!(
                ControlMessage::GetClipboard { copy_key }.to_bytes(),
                [0x08, byte]
            );
        }
    }

    #[test]
    fn serializes_set_clipboard() {
        assert_eq!(
            set_clipboard(0x0102_0304_0506_0708, true, "hello, world!").to_bytes(),
            hex("09 0102030405060708 01 0000000d 68656c6c6f2c20776f726c6421")
        );
        assert_eq!(
            set_clipboard(SEQUENCE_INVALID, false, "").to_bytes(),
            hex("09 0000000000000000 00 00000000")
        );
    }

    #[test]
    fn serializes_set_display_power() {
        assert_eq!(
            ControlMessage::SetDisplayPower { on: true }.to_bytes(),
            [0x0a, 0x01]
        );
        assert_eq!(
            ControlMessage::SetDisplayPower { on: false }.to_bytes(),
            [0x0a, 0x00]
        );
    }

    #[test]
    fn serializes_rotate_device() {
        assert_eq!(ControlMessage::RotateDevice.to_bytes(), [0x0b]);
    }

    #[test]
    fn serializes_reset_video() {
        assert_eq!(ControlMessage::ResetVideo.to_bytes(), [0x11]);
    }

    #[test]
    fn serialize_appends() {
        let mut buf = vec![0xaa];
        key(KeyAction::Down, KEYCODE_BACK).serialize(&mut buf);
        ControlMessage::ResetVideo.serialize(&mut buf);
        assert_eq!(buf, hex("aa 00 00 00000004 00000000 00000000 11"));
    }

    #[test]
    fn type_ids_match_the_wire_and_v5_0_1() {
        let position = Position {
            x: 0,
            y: 0,
            screen_width: 1,
            screen_height: 1,
        };
        let messages = [
            (key(KeyAction::Down, KEYCODE_HOME), 0),
            (text("a"), 1),
            (touch(MotionAction::Down, 0, 1.0, 0, (1, 1)), 2),
            (
                ControlMessage::InjectScroll {
                    position,
                    hscroll: 0.0,
                    vscroll: 1.0,
                    buttons: 0,
                },
                3,
            ),
            (
                ControlMessage::BackOrScreenOn {
                    action: KeyAction::Down,
                },
                4,
            ),
            (ControlMessage::ExpandNotificationPanel, 5),
            (ControlMessage::ExpandSettingsPanel, 6),
            (ControlMessage::CollapsePanels, 7),
            (
                ControlMessage::GetClipboard {
                    copy_key: CopyKey::None,
                },
                8,
            ),
            (set_clipboard(0, false, "a"), 9),
            (ControlMessage::SetDisplayPower { on: true }, 10),
            (ControlMessage::RotateDevice, 11),
            (ControlMessage::ResetVideo, 17),
        ];
        for (msg, id) in messages {
            assert_eq!(msg.type_id(), id, "{msg:?}");
            assert_eq!(msg.to_bytes()[0], id, "{msg:?}");
        }
        let unimplemented = [
            TYPE_UHID_CREATE,
            TYPE_UHID_INPUT,
            TYPE_UHID_DESTROY,
            TYPE_OPEN_HARD_KEYBOARD_SETTINGS,
            TYPE_START_APP,
            TYPE_CAMERA_SET_TORCH,
            TYPE_CAMERA_ZOOM_IN,
            TYPE_CAMERA_ZOOM_OUT,
            TYPE_RESIZE_DISPLAY,
            TYPE_SCAN_FILE,
        ];
        assert_eq!(unimplemented, [12, 13, 14, 15, 16, 18, 19, 20, 21, 22]);
    }

    // Edge cases.

    #[test]
    fn pressure_fixed_point() {
        assert_eq!(pressure_bytes(0.0), [0x00, 0x00]);
        assert_eq!(pressure_bytes(1.0), [0xff, 0xff]);
        assert_eq!(pressure_bytes(0.5), [0x80, 0x00]);
        assert_eq!(pressure_bytes(0.25), [0x40, 0x00]);
        assert_eq!(pressure_bytes(65535.0 / 65536.0), [0xff, 0xff]);
        assert_eq!(pressure_bytes(1.5), [0xff, 0xff], "clamped");
        assert_eq!(pressure_bytes(-0.5), [0x00, 0x00], "clamped");
        assert_eq!(pressure_bytes(f32::NAN), [0x00, 0x00]);
    }

    #[test]
    fn scroll_fixed_point_and_clamping() {
        assert_eq!(scroll_bytes(16.0), [0x7f, 0xff]);
        assert_eq!(scroll_bytes(-16.0), [0x80, 0x00]);
        assert_eq!(scroll_bytes(1.0), [0x08, 0x00]);
        assert_eq!(scroll_bytes(-1.0), [0xf8, 0x00]);
        assert_eq!(scroll_bytes(0.0), [0x00, 0x00]);
        assert_eq!(scroll_bytes(100.0), [0x7f, 0xff], "clamped to 16");
        assert_eq!(scroll_bytes(-100.0), [0x80, 0x00], "clamped to -16");
        assert_eq!(scroll_bytes(f32::INFINITY), [0x7f, 0xff]);
        assert_eq!(scroll_bytes(f32::NEG_INFINITY), [0x80, 0x00]);
        assert_eq!(scroll_bytes(f32::NAN), [0x00, 0x00]);
        // 1.5 / 2048 notches is exactly 1.5 units: truncated toward zero as upstream does, not
        // rounded.
        assert_eq!(scroll_bytes(1.5 / 2048.0), [0x00, 0x01]);
        assert_eq!(scroll_bytes(-1.5 / 2048.0), [0xff, 0xff]);
        let bytes = scroll(0.0, -0.5).to_bytes();
        assert_eq!(
            [bytes[15], bytes[16]],
            [0xfc, 0x00],
            "vscroll uses the same encoding"
        );
    }

    #[test]
    fn inject_text_maximum_length() {
        let max = "a".repeat(INJECT_TEXT_MAX_LENGTH);
        let bytes = text(&max).to_bytes();
        assert_eq!(bytes.len(), 5 + INJECT_TEXT_MAX_LENGTH);
        assert_eq!(bytes[..5], hex("01 0000012c"));

        let too_long = "a".repeat(INJECT_TEXT_MAX_LENGTH + 1);
        assert_eq!(text(&too_long).to_bytes(), bytes, "truncated to 300 bytes");

        // A character that would straddle the limit is left out whole.
        let straddling = format!("{}é", "a".repeat(INJECT_TEXT_MAX_LENGTH - 1));
        let bytes = text(&straddling).to_bytes();
        assert_eq!(bytes[..5], hex("01 0000012b"));
        assert_eq!(&bytes[5..], "a".repeat(299).as_bytes());

        let emoji = format!("{}😀", "a".repeat(INJECT_TEXT_MAX_LENGTH - 2));
        assert_eq!(
            text(&emoji).to_bytes().len(),
            5 + INJECT_TEXT_MAX_LENGTH - 2
        );
    }

    #[test]
    fn set_clipboard_maximum_length() {
        let max = "a".repeat(CLIPBOARD_TEXT_MAX_LENGTH);
        let bytes = set_clipboard(0x0102_0304_0506_0708, true, &max).to_bytes();
        assert_eq!(bytes.len(), MESSAGE_MAX_SIZE);
        assert_eq!(bytes[..14], hex("09 0102030405060708 01 0003fff2"));

        let too_long = format!("{max}a");
        assert_eq!(
            set_clipboard(0x0102_0304_0506_0708, true, &too_long).to_bytes(),
            bytes
        );

        let straddling = format!("{}€", "a".repeat(CLIPBOARD_TEXT_MAX_LENGTH - 2));
        let bytes = set_clipboard(0, false, &straddling).to_bytes();
        assert_eq!(bytes.len(), MESSAGE_MAX_SIZE - 2);
        assert!(bytes.ends_with(b"aa"));
    }

    #[test]
    fn text_chunks_split_on_character_boundaries() {
        assert_eq!(text_chunks("").count(), 0);
        assert_eq!(text_chunks("hello").collect::<Vec<_>>(), ["hello"]);

        let long = "a".repeat(650);
        let lengths: Vec<usize> = text_chunks(&long).map(str::len).collect();
        assert_eq!(lengths, [300, 300, 50]);

        let straddling = format!("{}éé", "a".repeat(299));
        let chunks: Vec<&str> = text_chunks(&straddling).collect();
        assert_eq!(chunks, ["a".repeat(299).as_str(), "éé"]);

        let mixed = "日本語のテキスト😀 and ASCII ".repeat(40);
        let chunks: Vec<&str> = text_chunks(&mixed).collect();
        assert!(
            chunks
                .iter()
                .all(|c| !c.is_empty() && c.len() <= INJECT_TEXT_MAX_LENGTH)
        );
        assert_eq!(chunks.concat(), mixed);
        for chunk in chunks {
            assert_eq!(
                text(chunk).to_bytes().len(),
                5 + chunk.len(),
                "never truncated"
            );
        }
    }

    #[test]
    fn parses_clipboard() {
        assert_eq!(
            parse_device_message(&hex("00 00000003 414243")),
            Ok(Some((DeviceMessage::Clipboard { text: "ABC".into() }, 8)))
        );
        assert_eq!(
            parse_device_message(&hex("00 00000000")),
            Ok(Some((
                DeviceMessage::Clipboard {
                    text: String::new()
                },
                5
            )))
        );
        assert_eq!(
            parse_device_message(&hex("00 00000003 41ff43")),
            Ok(Some((
                DeviceMessage::Clipboard {
                    text: "A\u{fffd}C".into()
                },
                8
            ))),
            "invalid UTF-8 is replaced"
        );
    }

    #[test]
    fn parses_ack_clipboard() {
        assert_eq!(
            parse_device_message(&hex("01 0102030405060708")),
            Ok(Some((
                DeviceMessage::AckClipboard {
                    sequence: 0x0102_0304_0506_0708
                },
                9
            )))
        );
    }

    #[test]
    fn parses_uhid_output() {
        assert_eq!(
            parse_device_message(&hex("02 002a 0005 0102030405")),
            Ok(Some((
                DeviceMessage::UhidOutput {
                    id: 42,
                    data: vec![1, 2, 3, 4, 5]
                },
                10
            )))
        );
        assert_eq!(
            parse_device_message(&hex("02 002a 0000")),
            Ok(Some((
                DeviceMessage::UhidOutput {
                    id: 42,
                    data: vec![]
                },
                5
            )))
        );
    }

    #[test]
    fn incomplete_device_messages_need_more_bytes() {
        assert_eq!(parse_device_message(&[]), Ok(None));
        for message in [
            hex("00 00000003 414243"),
            hex("01 0102030405060708"),
            hex("02 002a 0005 0102030405"),
        ] {
            for cut in 1..message.len() {
                assert_eq!(
                    parse_device_message(&message[..cut]),
                    Ok(None),
                    "{message:02x?} cut at {cut}"
                );
            }
        }
    }

    #[test]
    fn device_messages_back_to_back() {
        let buf = hex("01 0000000000000007 00 00000002 6869 02 0001 0001 ff 01");
        let (first, used) = parse_device_message(&buf).unwrap().unwrap();
        assert_eq!(
            (first, used),
            (DeviceMessage::AckClipboard { sequence: 7 }, 9)
        );
        let (second, used2) = parse_device_message(&buf[used..]).unwrap().unwrap();
        assert_eq!(second, DeviceMessage::Clipboard { text: "hi".into() });
        let rest = &buf[used + used2..];
        let (third, used3) = parse_device_message(rest).unwrap().unwrap();
        assert_eq!(
            third,
            DeviceMessage::UhidOutput {
                id: 1,
                data: vec![0xff]
            }
        );
        assert_eq!(
            parse_device_message(&rest[used3..]),
            Ok(None),
            "a lone type byte"
        );
    }

    #[test]
    fn clipboard_length_is_bounded() {
        let mut max = hex("00 0003fffb");
        max.resize(MESSAGE_MAX_SIZE, b'a');
        let (msg, used) = parse_device_message(&max).unwrap().unwrap();
        assert_eq!(used, MESSAGE_MAX_SIZE);
        assert_eq!(
            msg,
            DeviceMessage::Clipboard {
                text: "a".repeat(DEVICE_CLIPBOARD_TEXT_MAX_LENGTH)
            }
        );

        // Rejected from the header alone, without waiting for (or buffering) the text.
        assert_eq!(
            parse_device_message(&hex("00 0003fffc")),
            Err(ControlError::ClipboardTooLong(0x3fffc))
        );
        assert_eq!(
            parse_device_message(&hex("00 ffffffff")),
            Err(ControlError::ClipboardTooLong(u32::MAX))
        );
    }

    #[test]
    fn unknown_device_message_type_is_an_error() {
        for kind in [3, 0x7f, 0xff] {
            assert_eq!(
                parse_device_message(&[kind]),
                Err(ControlError::UnknownDeviceMessage(kind))
            );
            assert_eq!(
                parse_device_message(&[kind, 0, 0, 0, 0]),
                Err(ControlError::UnknownDeviceMessage(kind))
            );
        }
    }
}
