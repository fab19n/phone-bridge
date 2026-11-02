//! The framing every request uses, and parsers for the servers' text replies.
//!
//! A request is 4 hex digits of length plus the service string. The reply is `OKAY`, or `FAIL`
//! plus a length-prefixed message (`OVERVIEW.TXT`, now `docs/dev/overview.md`; `SendProtocolString`
//! and `ReadProtocolString` in `adb_io.cpp`, `adb_status` in `client/adb_client.cpp`).

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{Device, DeviceState, Error, ForwardRule, Result};

/// The longest service string 4 hex digits of length can describe.
const MAX_REQUEST: usize = 0xffff;

/// Every state `to_string(ConnectionState)` returns (`adb.cpp`) except `kCsNoPerm`, whose text
/// is `UsbNoPermissionsShortHelpText()` (`diagnose_usb.cpp`):
/// `no permissions (<reason>); see [<url>]`.
const STATES: [&str; 11] = [
    "device",
    "unauthorized",
    "offline",
    "authorizing",
    "connecting",
    "recovery",
    "rescue",
    "sideload",
    "bootloader",
    "host",
    "detached",
];
const NO_PERMISSIONS: &str = "no permissions";

/// `<length as 4 lowercase hex digits><service>`, as `SendProtocolString` writes it.
pub(crate) fn encode_request(service: &str) -> Result<Vec<u8>> {
    let len = service.len();
    if !(1..=MAX_REQUEST).contains(&len) {
        return Err(Error::Protocol(format!(
            "request of {len} bytes; the length field allows 1 to 65535"
        )));
    }
    let mut request = format!("{len:04x}").into_bytes();
    request.extend_from_slice(service.as_bytes());
    Ok(request)
}

/// Sends one request and reads its `OKAY`, or its `FAIL` as [`Error::Fail`].
pub(crate) async fn request<S>(stream: &mut S, service: &str) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    tracing::trace!(service, "adb request");
    stream.write_all(&encode_request(service)?).await?;
    read_status(stream).await
}

/// Reads `OKAY`, or `FAIL` plus its length-prefixed message as [`Error::Fail`].
pub(crate) async fn read_status<R: AsyncRead + Unpin>(reader: &mut R) -> Result<()> {
    let mut status = [0; 4];
    read_exact(reader, &mut status, "a status").await?;
    match &status {
        b"OKAY" => Ok(()),
        b"FAIL" => {
            let message = read_block(reader, "a FAIL message").await?;
            Err(Error::Fail(String::from_utf8_lossy(&message).into_owned()))
        }
        _ => Err(Error::Protocol(format!(
            "unexpected status \"{}\"",
            status.escape_ascii()
        ))),
    }
}

/// Reads 4 hex digits of length and that many bytes (at most 65535, so the allocation is bounded).
pub(crate) async fn read_block<R: AsyncRead + Unpin>(
    reader: &mut R,
    what: &str,
) -> Result<Vec<u8>> {
    read_block_or_eof(reader, what)
        .await?
        .ok_or_else(|| Error::Protocol(format!("connection closed before {what}")))
}

/// Like [`read_block`], but `None` if the stream ends before the length.
pub(crate) async fn read_block_or_eof<R: AsyncRead + Unpin>(
    reader: &mut R,
    what: &str,
) -> Result<Option<Vec<u8>>> {
    let mut digits = [0; 4];
    if !read_exact_or_eof(reader, &mut digits, what).await? {
        return Ok(None);
    }
    let mut block = vec![0; usize::from(parse_hex_len(digits)?)];
    if !read_exact_or_eof(reader, &mut block, what).await? {
        return Err(Error::Protocol(format!(
            "connection closed in the middle of {what}"
        )));
    }
    Ok(Some(block))
}

/// Splits one whole length-prefixed block off the front of `buf`, if it holds one.
pub(crate) fn take_block(buf: &mut Vec<u8>) -> Result<Option<Vec<u8>>> {
    let Some(&digits) = buf.first_chunk::<4>() else {
        return Ok(None);
    };
    let end = 4 + usize::from(parse_hex_len(digits)?);
    if buf.len() < end {
        return Ok(None);
    }
    let block = buf[4..end].to_vec();
    buf.drain(..end);
    Ok(Some(block))
}

/// Fills `buf`. `Ok(false)` if the stream ended before its first byte; ending part way is an error.
pub(crate) async fn read_exact_or_eof<R: AsyncRead + Unpin>(
    reader: &mut R,
    buf: &mut [u8],
    what: &str,
) -> Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = reader.read(&mut buf[filled..]).await?;
        if n == 0 {
            if filled == 0 {
                return Ok(false);
            }
            return Err(Error::Protocol(format!(
                "connection closed in the middle of {what}"
            )));
        }
        filled += n;
    }
    Ok(true)
}

/// Fills `buf`; any end of stream is an error.
pub(crate) async fn read_exact<R: AsyncRead + Unpin>(
    reader: &mut R,
    buf: &mut [u8],
    what: &str,
) -> Result<()> {
    if read_exact_or_eof(reader, buf, what).await? {
        Ok(())
    } else {
        Err(Error::Protocol(format!("connection closed before {what}")))
    }
}

/// A 4-hex-digit length, either case. Unlike AOSP's `strtoul`, a non-hex byte is an error.
pub(crate) fn parse_hex_len(digits: [u8; 4]) -> Result<u16> {
    parse_hex(&digits)
        .and_then(|n| u16::try_from(n).ok())
        .ok_or_else(|| Error::Protocol(format!("invalid length \"{}\"", digits.escape_ascii())))
}

/// 1 to 8 hex digits.
fn parse_hex(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() || digits.len() > 8 {
        return None;
    }
    digits
        .iter()
        .try_fold(0u32, |n, &d| Some((n << 4) | char::from(d).to_digit(16)?))
}

/// `host:version` replies `"%04x"` of `ADB_SERVER_VERSION` (`handle_host_request`): `0029` is 41.
pub(crate) fn parse_version(text: &str) -> Result<u32> {
    parse_hex(text.as_bytes())
        .ok_or_else(|| Error::Protocol(format!("invalid version \"{}\"", text.escape_default())))
}

pub(crate) fn parse_state(text: &str) -> DeviceState {
    match text {
        "device" => DeviceState::Device,
        "unauthorized" => DeviceState::Unauthorized,
        "offline" => DeviceState::Offline,
        "authorizing" => DeviceState::Authorizing,
        "connecting" => DeviceState::Connecting,
        "recovery" => DeviceState::Recovery,
        "sideload" => DeviceState::Sideload,
        "bootloader" => DeviceState::Bootloader,
        "host" => DeviceState::Host,
        _ if text.starts_with(NO_PERMISSIONS) => DeviceState::NoPermissions,
        _ => DeviceState::Unknown(text.to_owned()),
    }
}

/// `host:devices-l` (`append_transport` in `transport.cpp`): per device, `"%-22s %s"` of serial
/// and state, then ` <devpath>`, ` product:`, ` model:` and ` device:` when known, and
/// ` transport_id:`.
pub(crate) fn parse_devices_l(text: &str) -> Result<Vec<Device>> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_device_l)
        .collect()
}

fn parse_device_l(line: &str) -> Result<Device> {
    let (serial, state, attributes) = split_device_l(line)
        .ok_or_else(|| Error::Protocol(format!("unparsable device line \"{line}\"")))?;
    let mut device = Device {
        serial: serial.to_owned(),
        state,
        product: None,
        model: None,
        device: None,
        transport_id: None,
    };
    for attribute in attributes.split_whitespace() {
        match attribute.split_once(':') {
            Some(("product", value)) => device.product = Some(value.to_owned()),
            Some(("model", value)) => device.model = Some(value.to_owned()),
            Some(("device", value)) => device.device = Some(value.to_owned()),
            Some(("transport_id", value)) => {
                let id = value
                    .parse()
                    .map_err(|_| Error::Protocol(format!("invalid transport_id in \"{line}\"")))?;
                device.transport_id = Some(id);
            }
            // The devpath, e.g. `usb:1-4` or `3-1`.
            _ => {}
        }
    }
    Ok(device)
}

/// Splits a `devices-l` line into serial, state and attributes. A serial may contain spaces (mDNS
/// names like `adb-…-CDdefg (2)._adb-tls-connect._tcp`, scrcpy #6248), so the state is found from
/// the right, as scrcpy's `adb_parser.c` does. An unknown state falls back to the second token.
fn split_device_l(line: &str) -> Option<(&str, DeviceState, &str)> {
    if let Some(at) = line.find(" no permissions") {
        let serial = line[..at].trim_end();
        let rest = &line[at + 1 + NO_PERMISSIONS.len()..];
        let attributes = match rest.find(']') {
            Some(end) if rest.starts_with(" (") || rest.starts_with(';') => &rest[end + 1..],
            _ => rest,
        };
        return (!serial.is_empty()).then_some((serial, DeviceState::NoPermissions, attributes));
    }
    let mut head = line.trim_end();
    while let Some((before, token)) = head.rsplit_once(char::is_whitespace) {
        if STATES.contains(&token) {
            let serial = before.trim_end();
            if serial.is_empty() {
                return None;
            }
            return Some((serial, parse_state(token), &line[head.len()..]));
        }
        head = before.trim_end();
    }
    let (serial, rest) = line.trim().split_once(char::is_whitespace)?;
    let rest = rest.trim_start();
    let (state, attributes) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    Some((serial, parse_state(state), attributes))
}

/// `host:track-devices` lists: `<serial>\t<state>\n` per device (`append_transport`, short form).
pub(crate) fn parse_device_list(text: &str) -> Result<Vec<(String, DeviceState)>> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| match line.split_once('\t') {
            Some((serial, state)) if !serial.is_empty() && !state.is_empty() => {
                Ok((serial.to_owned(), parse_state(state)))
            }
            _ => Err(Error::Protocol(format!(
                "unparsable device line \"{line}\""
            ))),
        })
        .collect()
}

/// `features`: comma-separated names (`FeatureSetToString` in `transport.cpp`).
pub(crate) fn parse_features(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `host:list-forward`: `<serial> <local> <remote>\n` per rule (`format_listeners` in
/// `adb_listeners.cpp`). Split from the right, because a serial may contain spaces.
pub(crate) fn parse_forwards(text: &str) -> Result<Vec<ForwardRule>> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.rsplitn(3, ' ');
            match (fields.next(), fields.next(), fields.next()) {
                (Some(remote), Some(local), Some(serial))
                    if !remote.is_empty() && !local.is_empty() && !serial.is_empty() =>
                {
                    Ok(ForwardRule {
                        serial: serial.to_owned(),
                        local: local.to_owned(),
                        remote: remote.to_owned(),
                    })
                }
                _ => Err(Error::Protocol(format!(
                    "unparsable forward rule \"{line}\""
                ))),
            }
        })
        .collect()
}

/// The decimal port that `forward:` reports for a new `tcp:` listener (`handle_forward_request`).
pub(crate) fn parse_port(text: &str) -> Result<u16> {
    match text.parse::<u16>() {
        Ok(port) if port != 0 && text.bytes().all(|b| b.is_ascii_digit()) => Ok(port),
        _ => Err(Error::Protocol(format!(
            "invalid forward port \"{}\"",
            text.escape_default()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_protocol<T>(result: &Result<T>) -> bool {
        matches!(result, Err(Error::Protocol(_)))
    }

    #[test]
    fn request_is_hex_length_then_service() {
        assert_eq!(encode_request("host:version").unwrap(), b"000chost:version");
        assert_eq!(
            encode_request("host:transport:R58M123ABC").unwrap(),
            b"0019host:transport:R58M123ABC"
        );
    }

    #[test]
    fn request_length_must_fit_four_hex_digits() {
        assert!(is_protocol(&encode_request("")));
        let longest = encode_request(&"a".repeat(0xffff)).unwrap();
        assert_eq!(&longest[..4], b"ffff");
        assert_eq!(longest.len(), 4 + 0xffff);
        assert!(is_protocol(&encode_request(&"a".repeat(0x1_0000))));
    }

    #[test]
    fn hex_length_takes_either_case_and_nothing_else() {
        assert_eq!(parse_hex_len(*b"0000").unwrap(), 0);
        assert_eq!(parse_hex_len(*b"001a").unwrap(), 26);
        assert_eq!(parse_hex_len(*b"FFFF").unwrap(), 0xffff);
        for bad in [b"00zz", b"+123", b" 123", b"12\n4", b"0x12", b"-001"] {
            assert!(is_protocol(&parse_hex_len(*bad)), "{}", bad.escape_ascii());
        }
    }

    async fn status(mut bytes: &[u8]) -> Result<()> {
        read_status(&mut bytes).await
    }

    #[tokio::test]
    async fn status_is_okay_or_fail_with_message() {
        status(b"OKAY").await.unwrap();
        let fail = status(b"FAIL0014device 'x' not found").await;
        assert!(
            matches!(&fail, Err(Error::Fail(m)) if m == "device 'x' not found"),
            "{fail:?}"
        );
        // The forward transport lookup in `handle_host_request` fails with an empty message.
        assert!(matches!(status(b"FAIL0000").await, Err(Error::Fail(m)) if m.is_empty()));
    }

    #[tokio::test]
    async fn malformed_or_short_statuses_are_protocol_errors() {
        let cases: [&[u8]; 9] = [
            b"",
            b"OK",
            b"OKAX",
            b"okay",
            b"FAIL",
            b"FAIL00",
            b"FAIL00zzoops",
            b"FAIL0010short",
            b"FAILffff",
        ];
        for bad in cases {
            assert!(is_protocol(&status(bad).await), "{}", bad.escape_ascii());
        }
    }

    #[tokio::test]
    async fn status_read_stops_at_the_status() {
        let mut bytes: &[u8] = b"OKAY\0raw";
        read_status(&mut bytes).await.unwrap();
        assert_eq!(bytes, b"\0raw");
    }

    #[tokio::test]
    async fn blocks_are_read_by_their_length() {
        let mut bytes: &[u8] = b"0004abcdEXTRA";
        assert_eq!(read_block(&mut bytes, "a reply").await.unwrap(), b"abcd");
        assert_eq!(bytes, b"EXTRA");

        let mut empty: &[u8] = b"0000";
        assert_eq!(read_block(&mut empty, "a reply").await.unwrap(), b"");

        // The largest length is 65535; a peer that sends less is an error, not a hang or a panic.
        let mut short: &[u8] = b"ffffonly a few bytes";
        assert!(is_protocol(&read_block(&mut short, "a reply").await));

        let mut eof: &[u8] = b"";
        assert_eq!(read_block_or_eof(&mut eof, "a reply").await.unwrap(), None);
        let mut cut: &[u8] = b"00";
        assert!(is_protocol(&read_block_or_eof(&mut cut, "a reply").await));
        let mut bad: &[u8] = b"12g4abcd";
        assert!(is_protocol(&read_block_or_eof(&mut bad, "a reply").await));
    }

    #[test]
    fn take_block_waits_for_a_whole_block() {
        let mut buf = b"0005ab".to_vec();
        assert_eq!(take_block(&mut buf).unwrap(), None);
        buf.extend_from_slice(b"cde00000");
        assert_eq!(take_block(&mut buf).unwrap(), Some(b"abcde".to_vec()));
        assert_eq!(take_block(&mut buf).unwrap(), Some(Vec::new()));
        assert_eq!(buf, b"0");
        assert_eq!(take_block(&mut buf).unwrap(), None);
        assert!(is_protocol(&take_block(&mut b"zzzz".to_vec())));
    }

    #[test]
    fn version_is_hex() {
        assert_eq!(parse_version("0029").unwrap(), 41);
        assert_eq!(parse_version("001f").unwrap(), 31);
        for bad in ["", "+29", "29 ", "0x29", "123456789"] {
            assert!(is_protocol(&parse_version(bad)), "{bad:?}");
        }
    }

    #[test]
    fn states() {
        assert_eq!(parse_state("device"), DeviceState::Device);
        assert_eq!(parse_state("unauthorized"), DeviceState::Unauthorized);
        assert_eq!(parse_state("offline"), DeviceState::Offline);
        assert_eq!(parse_state("authorizing"), DeviceState::Authorizing);
        assert_eq!(parse_state("connecting"), DeviceState::Connecting);
        assert_eq!(parse_state("recovery"), DeviceState::Recovery);
        assert_eq!(parse_state("sideload"), DeviceState::Sideload);
        assert_eq!(parse_state("bootloader"), DeviceState::Bootloader);
        assert_eq!(parse_state("host"), DeviceState::Host);
        assert_eq!(
            parse_state("no permissions; see [http://developer.android.com/tools/device.html]"),
            DeviceState::NoPermissions
        );
        assert_eq!(parse_state("rescue"), DeviceState::Unknown("rescue".into()));
    }

    /// One line as `append_transport` writes it: `"%-22s %s"`, then the rest.
    fn line(serial: &str, rest: &str) -> String {
        format!("{serial:<22} {rest}\n")
    }

    fn device(serial: &str, state: DeviceState) -> Device {
        Device {
            serial: serial.into(),
            state,
            product: None,
            model: None,
            device: None,
            transport_id: None,
        }
    }

    #[test]
    fn devices_l_realistic_lines() {
        let text = [
            line(
                "R58M123ABC",
                "device usb:1-4 product:beyond1qltesq model:SM_G973U device:beyond1q transport_id:3",
            ),
            line("0123456789ABCDEF", "unauthorized usb:1-2 transport_id:4"),
            line(
                "ZY22ABCDEF",
                "no permissions (missing udev rules? user is in the plugdev group); \
                 see [http://developer.android.com/tools/device.html] usb:1-1 transport_id:5",
            ),
            line(
                "ZY22GHIJKL",
                "no permissions; see [http://developer.android.com/tools/device.html] usb:1-3 transport_id:8",
            ),
            line(
                "emulator-5554",
                "device product:sdk_gphone64_x86_64 model:sdk_gphone64_x86_64 device:emu64xa transport_id:1",
            ),
            line(
                "adb-R58M123ABC-AbCdEf (2)._adb-tls-connect._tcp",
                "device product:beyond1qltesq model:SM_G973U device:beyond1q transport_id:7",
            ),
            line("192.168.1.20:5555", "offline transport_id:6"),
            line("(no serial number)", "device usb:1-5 transport_id:11"),
            line("12345678", "device 3-1 product:manet model:23117RK66C device:manet transport_id:2"),
            line("R3SCU3", "rescue usb:1-6 transport_id:12"),
            line("F00BA4", "sideload usb:1-7 transport_id:13"),
            line("NEWSTATE1", "hibernating usb:1-8 transport_id:14"),
        ]
        .concat();
        let devices = parse_devices_l(&text).unwrap();

        let samsung = Device {
            product: Some("beyond1qltesq".into()),
            model: Some("SM_G973U".into()),
            device: Some("beyond1q".into()),
            transport_id: Some(3),
            ..device("R58M123ABC", DeviceState::Device)
        };
        let expected = vec![
            samsung.clone(),
            Device {
                transport_id: Some(4),
                ..device("0123456789ABCDEF", DeviceState::Unauthorized)
            },
            Device {
                transport_id: Some(5),
                ..device("ZY22ABCDEF", DeviceState::NoPermissions)
            },
            Device {
                transport_id: Some(8),
                ..device("ZY22GHIJKL", DeviceState::NoPermissions)
            },
            Device {
                product: Some("sdk_gphone64_x86_64".into()),
                model: Some("sdk_gphone64_x86_64".into()),
                device: Some("emu64xa".into()),
                transport_id: Some(1),
                ..device("emulator-5554", DeviceState::Device)
            },
            Device {
                serial: "adb-R58M123ABC-AbCdEf (2)._adb-tls-connect._tcp".into(),
                transport_id: Some(7),
                ..samsung
            },
            Device {
                transport_id: Some(6),
                ..device("192.168.1.20:5555", DeviceState::Offline)
            },
            Device {
                transport_id: Some(11),
                ..device("(no serial number)", DeviceState::Device)
            },
            Device {
                product: Some("manet".into()),
                model: Some("23117RK66C".into()),
                device: Some("manet".into()),
                transport_id: Some(2),
                ..device("12345678", DeviceState::Device)
            },
            Device {
                transport_id: Some(12),
                ..device("R3SCU3", DeviceState::Unknown("rescue".into()))
            },
            Device {
                transport_id: Some(13),
                ..device("F00BA4", DeviceState::Sideload)
            },
            Device {
                transport_id: Some(14),
                ..device("NEWSTATE1", DeviceState::Unknown("hibernating".into()))
            },
        ];
        assert_eq!(devices, expected);
    }

    #[test]
    fn devices_l_edge_cases() {
        assert_eq!(parse_devices_l("").unwrap(), Vec::new());
        // Older servers: no reason, no URL, no transport id.
        assert_eq!(
            parse_devices_l("????????????           no permissions\n").unwrap(),
            vec![device("????????????", DeviceState::NoPermissions)]
        );
        assert!(is_protocol(&parse_devices_l("lonelytoken\n")));
        assert!(is_protocol(&parse_devices_l(" no permissions usb:1-1\n")));
        assert!(is_protocol(&parse_devices_l(
            "R58M123ABC device transport_id:x\n"
        )));
    }

    #[test]
    fn track_devices_short_lines() {
        let text = "R58M123ABC\tdevice\n\
                    emulator-5554\toffline\n\
                    ZY22ABCDEF\tno permissions (missing udev rules? user is in the plugdev group); \
                    see [http://developer.android.com/tools/device.html]\n\
                    (no serial number)\tunauthorized\n\
                    adb-R58M123ABC-AbCdEf (2)._adb-tls-connect._tcp\tauthorizing\n";
        assert_eq!(
            parse_device_list(text).unwrap(),
            vec![
                ("R58M123ABC".to_owned(), DeviceState::Device),
                ("emulator-5554".to_owned(), DeviceState::Offline),
                ("ZY22ABCDEF".to_owned(), DeviceState::NoPermissions),
                ("(no serial number)".to_owned(), DeviceState::Unauthorized),
                (
                    "adb-R58M123ABC-AbCdEf (2)._adb-tls-connect._tcp".to_owned(),
                    DeviceState::Authorizing
                ),
            ]
        );
        assert_eq!(parse_device_list("").unwrap(), Vec::new());
        assert!(is_protocol(&parse_device_list("R58M123ABC device\n")));
        assert!(is_protocol(&parse_device_list("\tdevice\n")));
        assert!(is_protocol(&parse_device_list("R58M123ABC\t\n")));
    }

    #[test]
    fn features_split_on_commas() {
        assert_eq!(
            parse_features("shell_v2,cmd,stat_v2,ls_v2"),
            ["shell_v2", "cmd", "stat_v2", "ls_v2"]
        );
        assert_eq!(parse_features(""), Vec::<String>::new());
    }

    #[test]
    fn forward_rules() {
        let text = "R58M123ABC tcp:27183 localabstract:scrcpy_1a2b3c4d\n\
                    adb-R58M123ABC-AbCdEf (2)._adb-tls-connect._tcp tcp:5000 tcp:6000\n";
        assert_eq!(
            parse_forwards(text).unwrap(),
            vec![
                ForwardRule {
                    serial: "R58M123ABC".into(),
                    local: "tcp:27183".into(),
                    remote: "localabstract:scrcpy_1a2b3c4d".into(),
                },
                ForwardRule {
                    serial: "adb-R58M123ABC-AbCdEf (2)._adb-tls-connect._tcp".into(),
                    local: "tcp:5000".into(),
                    remote: "tcp:6000".into(),
                },
            ]
        );
        assert_eq!(parse_forwards("").unwrap(), Vec::new());
        assert!(is_protocol(&parse_forwards("R58M123ABC tcp:27183\n")));
    }

    #[test]
    fn forward_port_is_decimal() {
        assert_eq!(parse_port("27183").unwrap(), 27183);
        for bad in ["", "0", "+1", "65536", "27183\n", "abc"] {
            assert!(is_protocol(&parse_port(bad)), "{bad:?}");
        }
    }
}
