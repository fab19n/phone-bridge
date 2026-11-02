//! Tests 0A, 0B and 0D: run the real stream pipeline, from the scrcpy video stream through
//! phonebridge-scrcpy and the video channel to WebCodecs on the page. `spike_replay` reads a fixture, so
//! no phone is needed. `spike_mirror` runs scrcpy-server on a device (`protocol/video.md` §13) and can
//! record the stream as a new fixture.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use phonebridge_adb::{AdbServer, DeviceState, ShellEvent};
use phonebridge_scrcpy::server::{ServerOptions, scid_from, socket_name};
use phonebridge_scrcpy::video::{self, Codec, Header, VideoEvent, VideoStream};
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, InvokeResponseBody};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::adb_probe::adb_pids;
use crate::video_channel;

const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures");
const SERVER_JAR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/binaries/scrcpy-server.jar");
const SERVER_LOG_LINES: usize = 200;
/// How long the server may take to start listening and send the dummy byte (scrcpy's client allows
/// 100 attempts of 100 ms).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamReport {
    codec: String,
    sessions: Vec<(u32, u32)>,
    decoder_configs: Vec<String>,
    frames: u32,
    key_frames: u32,
    payload_bytes: u64,
    messages_sent: u32,
    elapsed_ms: u64,
}

#[tauri::command]
pub async fn spike_replay(
    fixture: String,
    realtime: bool,
    on_video: Channel<InvokeResponseBody>,
) -> Result<StreamReport, String> {
    let bytes = tokio::fs::read(fixture_path(&fixture, false)?)
        .await
        .map_err(|e| e.to_string())?;
    let mut reader = std::io::Cursor::new(bytes);
    let codec = video::read_codec(&mut reader)
        .await
        .map_err(|e| e.to_string())?;
    pump(&mut reader, codec, &on_video, realtime, None, None).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorOptions {
    serial: Option<String>,
    seconds: u32,
    max_size: u32,
    video_bit_rate: u32,
    /// Record the stream to `fixtures/local/<name>.bin`.
    record_as: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorReport {
    serial: String,
    device_name: String,
    connect_attempts: u32,
    connect_ms: u64,
    stream: StreamReport,
    server_log: Vec<String>,
    server_exit: Option<u8>,
    recorded_to: Option<String>,
    /// adb.exe process ids before and after the session, for test 0G with a device attached.
    adb_pids_before: Vec<u32>,
    adb_pids_after: Vec<u32>,
    /// The same adb server process ran before and after the session.
    server_untouched: bool,
}

#[tauri::command]
pub async fn spike_mirror(
    options: MirrorOptions,
    on_video: Channel<InvokeResponseBody>,
) -> Result<MirrorReport, String> {
    let adb_pids_before = adb_pids();
    let server = AdbServer::from_env();
    let serial = pick_device(&server, options.serial.as_deref()).await?;
    let jar = tokio::fs::read(SERVER_JAR)
        .await
        .map_err(|e| format!("{SERVER_JAR}: {e} (run scripts/fetch-scrcpy-server.ps1)"))?;
    if !phonebridge_scrcpy::is_pinned_server(&jar) {
        return Err("scrcpy-server.jar does not match the pinned SHA-256".into());
    }
    let mtime = (video_channel::now_us() / 1_000_000) as u32;
    server
        .push(
            &serial,
            &jar,
            phonebridge_scrcpy::SERVER_DEVICE_PATH,
            0o644,
            mtime,
        )
        .await
        .map_err(|e| format!("push: {e}"))?;

    let scid = scid_from(rand::random());
    let mut server_options = ServerOptions::new(scid);
    server_options.max_size = options.max_size;
    server_options.video_bit_rate = options.video_bit_rate;
    let features = server.features(&serial).await.map_err(|e| e.to_string())?;
    if !features.iter().any(|f| f == "shell_v2") {
        return Err(
            "the device lacks shell_v2; this spike doesn't implement the legacy shell path".into(),
        );
    }
    let command = server_options.command().map_err(|e| e.to_string())?;
    let mut shell = server
        .shell_v2(&serial, &command)
        .await
        .map_err(|e| format!("starting the server: {e}"))?;

    // The shell stream carries the server's log and exit code; it must be read continuously. It
    // ending for any reason (exit frame, EOF, error) means the server is gone.
    let log = Arc::new(Mutex::new(Vec::<String>::new()));
    let exit = Arc::new(Mutex::new(None::<u8>));
    let shell_ended = Arc::new(AtomicBool::new(false));
    let shell_task = {
        let (log, exit, shell_ended) = (log.clone(), exit.clone(), shell_ended.clone());
        tauri::async_runtime::spawn(async move {
            while let Ok(Some(event)) = shell.next().await {
                match event {
                    ShellEvent::Stdout(bytes) | ShellEvent::Stderr(bytes) => push_log(&log, &bytes),
                    ShellEvent::Exit(code) => {
                        if let Ok(mut slot) = exit.lock() {
                            *slot = Some(code);
                        }
                        break;
                    }
                }
            }
            shell_ended.store(true, Ordering::SeqCst);
        })
    };
    let server_gone = || shell_ended.load(Ordering::SeqCst);

    let service = format!("localabstract:{}", socket_name(scid));
    let session = run_session(
        &server,
        &serial,
        &service,
        &options,
        &on_video,
        &server_gone,
    )
    .await;

    // run_session has dropped its sockets, so the server exits on the broken pipe. Give it a second,
    // then end the shell session either way.
    let wait_start = Instant::now();
    while !server_gone() && wait_start.elapsed() < Duration::from_secs(1) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    shell_task.abort();
    let server_exit = exit.lock().map(|slot| *slot).unwrap_or(None);
    let adb_pids_after = adb_pids();

    let session =
        session.map_err(|e| format!("{e}; server log: {}", snapshot(&log).join(" | ")))?;
    Ok(MirrorReport {
        serial,
        device_name: session.device_name,
        connect_attempts: session.connect_attempts,
        connect_ms: session.connect_ms,
        stream: session.stream,
        server_log: snapshot(&log),
        server_exit,
        recorded_to: session.recorded_to,
        server_untouched: !adb_pids_before.is_empty() && adb_pids_before == adb_pids_after,
        adb_pids_before,
        adb_pids_after,
    })
}

struct Session {
    device_name: String,
    connect_attempts: u32,
    connect_ms: u64,
    stream: StreamReport,
    recorded_to: Option<String>,
}

/// Connects the video and control sockets, then streams until the deadline. The sockets close when
/// this returns.
async fn run_session(
    server: &AdbServer,
    serial: &str,
    service: &str,
    options: &MirrorOptions,
    channel: &Channel<InvokeResponseBody>,
    server_gone: &impl Fn() -> bool,
) -> Result<Session, String> {
    // Video socket first (§13 step 6). Until the server listens, adb answers FAIL "closed", so
    // retry that and nothing else. Once open() succeeds, the server has accepted this connection
    // as its video socket: wait for the dummy byte rather than reconnecting, because the next
    // connection it accepts becomes the control socket.
    let connect_start = Instant::now();
    let deadline = connect_start + CONNECT_TIMEOUT;
    let mut connect_attempts = 0;
    let mut video_socket = loop {
        connect_attempts += 1;
        match server.open(serial, service).await {
            Ok(socket) => break socket,
            Err(phonebridge_adb::Error::Fail(message)) if message.contains("closed") => {
                if server_gone() {
                    return Err(format!(
                        "the server ended before it listened (attempt {connect_attempts}: {message})"
                    ));
                }
                if Instant::now() >= deadline {
                    return Err(format!(
                        "the server didn't listen within {CONNECT_TIMEOUT:?} \
                         ({connect_attempts} attempts; last reply: {message})"
                    ));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => return Err(format!("opening the video socket: {e}")),
        }
    };
    let mut dummy = [0u8; 1];
    match tokio::time::timeout_at(deadline.into(), video_socket.read_exact(&mut dummy)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(format!("reading the dummy byte: {e}")),
        Err(_) => return Err(format!("no dummy byte within {CONNECT_TIMEOUT:?}")),
    }
    let connect_ms = connect_start.elapsed().as_millis() as u64;
    // The device meta only arrives once the control socket is connected too (§5).
    let _control_socket = server
        .open(serial, service)
        .await
        .map_err(|e| format!("control socket: {e}"))?;
    let device_name = video::read_device_name(&mut video_socket)
        .await
        .map_err(|e| e.to_string())?;
    let codec = video::read_codec(&mut video_socket)
        .await
        .map_err(|e| e.to_string())?;

    let mut recording = None;
    if let Some(name) = &options.record_as {
        let path = fixture_path(name, true)?;
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(|e| e.to_string())?;
        }
        let mut file = tokio::fs::File::create(&path)
            .await
            .map_err(|e| e.to_string())?;
        file.write_all(&codec_id(codec))
            .await
            .map_err(|e| e.to_string())?;
        recording = Some((file, path));
    }

    let deadline = Instant::now() + Duration::from_secs(options.seconds.clamp(1, 600).into());
    let stream = pump(
        &mut video_socket,
        codec,
        channel,
        false,
        recording.as_mut().map(|(file, _)| file),
        Some(deadline),
    )
    .await?;

    let recorded_to = match recording {
        Some((mut file, path)) => {
            file.flush().await.map_err(|e| e.to_string())?;
            Some(path.display().to_string())
        }
        None => None,
    };
    Ok(Session {
        device_name,
        connect_attempts,
        connect_ms,
        stream,
        recorded_to,
    })
}

/// Reads packets until EOF (or `deadline`) and sends decoder-config records and frames to the page.
async fn pump<R: AsyncRead + Unpin>(
    reader: &mut R,
    codec: Codec,
    channel: &Channel<InvokeResponseBody>,
    realtime: bool,
    mut record: Option<&mut tokio::fs::File>,
    deadline: Option<Instant>,
) -> Result<StreamReport, String> {
    let mut report = StreamReport {
        codec: format!("{codec:?}"),
        ..Default::default()
    };
    let mut stream = VideoStream::new(codec);
    // PacketReader keeps a partly read packet across a timed-out `next()`, unlike read_packet.
    let mut packets = video::PacketReader::new(reader);
    let start = Instant::now();
    let mut first_pts = None;
    loop {
        let packet = match deadline {
            Some(deadline) => {
                match tokio::time::timeout_at(deadline.into(), packets.next()).await {
                    Ok(packet) => packet,
                    Err(_) => break, // time is up
                }
            }
            None => packets.next().await,
        };
        let Some((header, payload)) = packet.map_err(|e| e.to_string())? else {
            break;
        };
        if let Some(file) = record.as_deref_mut() {
            file.write_all(&encode_header(header))
                .await
                .map_err(|e| e.to_string())?;
            file.write_all(&payload).await.map_err(|e| e.to_string())?;
        }
        for event in stream.push(header, payload).map_err(|e| e.to_string())? {
            let message = match event {
                VideoEvent::Session { width, height } => {
                    report.sessions.push((width, height));
                    continue;
                }
                VideoEvent::DecoderConfig {
                    codec,
                    width,
                    height,
                } => {
                    let message = video_channel::decoder_config_message(
                        &codec,
                        width,
                        height,
                        video_channel::now_us(),
                    );
                    report.decoder_configs.push(codec);
                    message
                }
                VideoEvent::Frame { pts_us, key, data } => {
                    if realtime {
                        let first = *first_pts.get_or_insert(pts_us);
                        let due = start + Duration::from_micros(pts_us.saturating_sub(first));
                        tokio::time::sleep_until(due.into()).await;
                    }
                    report.frames += 1;
                    report.key_frames += u32::from(key);
                    report.payload_bytes += data.len() as u64;
                    video_channel::frame_message(key, pts_us, video_channel::now_us(), &data)
                }
            };
            channel
                .send(InvokeResponseBody::Raw(message))
                .map_err(|e| e.to_string())?;
            report.messages_sent += 1;
        }
    }
    report.elapsed_ms = start.elapsed().as_millis() as u64;
    Ok(report)
}

async fn pick_device(server: &AdbServer, wanted: Option<&str>) -> Result<String, String> {
    let devices = server.devices().await.map_err(|e| format!("adb: {e}"))?;
    let ready: Vec<_> = devices
        .iter()
        .filter(|d| d.state == DeviceState::Device)
        .collect();
    match (wanted, ready.as_slice()) {
        (Some(serial), _) => ready
            .iter()
            .find(|d| d.serial == serial)
            .map(|d| d.serial.clone())
            .ok_or_else(|| {
                format!("{serial} is not connected and authorized (devices: {devices:?})")
            }),
        (None, [only]) => Ok(only.serial.clone()),
        (None, []) => Err(format!("no authorized device (devices: {devices:?})")),
        (None, _) => Err("several devices are connected; pass a serial".into()),
    }
}

/// Fixture names are plain file stems inside `fixtures/` (or `fixtures/local/`), so the page can't
/// make Rust read or write arbitrary paths.
fn fixture_path(name: &str, local: bool) -> Result<PathBuf, String> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("invalid fixture name {name:?}"));
    }
    let mut path = PathBuf::from(FIXTURES_DIR);
    if local {
        path.push("local");
    }
    path.push(format!("{name}.bin"));
    Ok(path)
}

/// The 4-byte codec id that starts a fixture (`protocol/video.md` §7).
fn codec_id(codec: Codec) -> [u8; 4] {
    match codec {
        Codec::H264 => *b"h264",
        Codec::H265 => *b"h265",
        Codec::Av1 => [0, b'a', b'v', b'1'],
    }
}

/// Re-encodes a parsed header byte for byte (`protocol/video.md` §8–9), to record fixtures.
fn encode_header(header: Header) -> [u8; video::HEADER_LEN] {
    let mut raw = [0u8; video::HEADER_LEN];
    match header {
        Header::Session {
            width,
            height,
            client_resized,
        } => {
            raw[..4].copy_from_slice(&(0x8000_0000u32 | u32::from(client_resized)).to_be_bytes());
            raw[4..8].copy_from_slice(&width.to_be_bytes());
            raw[8..].copy_from_slice(&height.to_be_bytes());
        }
        Header::Media {
            config,
            key,
            pts_us,
            len,
        } => {
            let mut pts_and_flags = if config { 1u64 << 62 } else { pts_us };
            if key {
                pts_and_flags |= 1 << 61;
            }
            raw[..8].copy_from_slice(&pts_and_flags.to_be_bytes());
            raw[8..].copy_from_slice(&len.to_be_bytes());
        }
    }
    raw
}

fn push_log(log: &Mutex<Vec<String>>, bytes: &[u8]) {
    if let Ok(mut lines) = log.lock() {
        lines.extend(String::from_utf8_lossy(bytes).lines().map(str::to_owned));
        let excess = lines.len().saturating_sub(SERVER_LOG_LINES);
        lines.drain(..excess);
    }
}

fn snapshot(log: &Mutex<Vec<String>>) -> Vec<String> {
    log.lock().map(|lines| lines.clone()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &[u8] = include_bytes!("../../../../fixtures/synthetic-rotation.bin");

    #[tokio::test]
    async fn recorder_reproduces_the_fixture_byte_for_byte() {
        let mut reader = std::io::Cursor::new(FIXTURE);
        let codec = video::read_codec(&mut reader).await.unwrap();
        let mut rebuilt = codec_id(codec).to_vec();
        while let Some((header, payload)) = video::read_packet(&mut reader).await.unwrap() {
            rebuilt.extend_from_slice(&encode_header(header));
            rebuilt.extend_from_slice(&payload);
        }
        assert_eq!(rebuilt, FIXTURE);
    }

    #[test]
    fn fixture_names_cannot_escape_the_fixtures_dir() {
        assert!(fixture_path("synthetic-rotation", false).is_ok());
        for bad in ["", "../secrets", "a/b", "a\\b", "c:", "x.bin"] {
            assert!(fixture_path(bad, true).is_err(), "{bad:?} accepted");
        }
    }
}
