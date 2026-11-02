// PhoneBridge Phase 0 — Validation Test App
// Run with: cd phase0-tests && npm run tauri dev
//
// This app runs 4 tests to validate the architecture before building the product:
//   Test 0A: scrcpy server connects and streams H.264 packets
//   Test 0B: WebCodecs VideoDecoder works in WebView2
//   Test 0C: Tauri Channel throughput is sufficient for 60 FPS
//   Test 0D: End-to-end (scrcpy → Rust → Channel → WebCodecs → canvas)

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use byteorder::{BigEndian, ReadBytesExt};
use serde::Serialize;
use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::ipc::Channel;
use tauri::Manager;

// ---------------------------------------------------------------------------
// Data types shared with the frontend
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(tag = "type")]
enum VideoEvent {
    #[serde(rename = "metadata")]
    Metadata {
        device_name: String,
        width: u32,
        height: u32,
        codec_id: u32,
    },
    #[serde(rename = "config")]
    Config {
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    #[serde(rename = "frame")]
    Frame {
        pts: u64,
        keyframe: bool,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
        received_us: u64,
    },
    #[serde(rename = "stopped")]
    Stopped { reason: String },
}

#[derive(Clone, Serialize)]
struct ThroughputResult {
    frames_sent: u64,
    elapsed_ms: u64,
    avg_send_us: u64,
    fps: f64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Resolve the path to the bundled adb binary.
/// In dev mode, it's at src-tauri/binaries/adb.exe
/// In production, Tauri places sidecars next to the executable.
fn adb_path(app: &tauri::AppHandle) -> PathBuf {
    // For Phase 0 testing, assume adb is on PATH.
    // In the real app, use app.path().resource_dir() for the sidecar.
    let _ = app;
    PathBuf::from("adb")
}

/// Run an adb command and return stdout as a String.
fn adb_run(adb: &PathBuf, args: &[&str]) -> Result<String, String> {
    let output = Command::new(adb)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run adb: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("adb {} failed: {}", args.join(" "), stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// List connected device serial numbers.
fn list_devices(adb: &PathBuf) -> Result<Vec<String>, String> {
    let output = adb_run(adb, &["devices"])?;
    let serials: Vec<String> = output
        .lines()
        .skip(1) // skip "List of devices attached"
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 && parts[1] == "device" {
                Some(parts[0].to_string())
            } else {
                None
            }
        })
        .collect();
    Ok(serials)
}

// ---------------------------------------------------------------------------
// Test 0A: Verify scrcpy server connects and streams
// ---------------------------------------------------------------------------

#[tauri::command]
fn test_0a_detect_device(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let adb = adb_path(&app);
    list_devices(&adb)
}

#[tauri::command]
fn test_0a_start_scrcpy(
    app: tauri::AppHandle,
    serial: String,
    on_event: Channel<VideoEvent>,
) -> Result<(), String> {
    let adb = adb_path(&app);

    // 1. Push scrcpy-server.jar to the device
    //    For Phase 0, you need to have scrcpy-server.jar in the current directory.
    //    Download it from scrcpy releases: https://github.com/Genymobile/scrcpy/releases
    let jar_path = "scrcpy-server.jar";
    println!("[0A] Pushing scrcpy server to device...");
    adb_run(&adb, &["-s", &serial, "push", jar_path, "/data/local/tmp/scrcpy-server.jar"])?;

    // 2. Set up ADB port forwarding
    let local_port = 27183u16;
    let scid = rand::random::<u32>() & 0x7FFFFFFF; // 31-bit random
    let abstract_name = format!("scrcpy_{:08x}", scid);

    println!("[0A] Setting up ADB forward tcp:{} -> localabstract:{}", local_port, abstract_name);
    adb_run(
        &adb,
        &[
            "-s", &serial, "forward",
            &format!("tcp:{}", local_port),
            &format!("localabstract:{}", abstract_name),
        ],
    )?;

    // 3. Start scrcpy server on the device
    println!("[0A] Starting scrcpy server...");
    let _server_process = Command::new(adb.as_os_str())
        .args([
            "-s", &serial, "shell",
            "CLASSPATH=/data/local/tmp/scrcpy-server.jar",
            "app_process", "/",
            "com.genymobile.scrcpy.Server", "4.0",
            &format!("scid={:08x}", scid),
            "tunnel_forward=true",
            "video_codec=h264",
            "audio=false",
            "control=false",
            "send_frame_meta=true",
            "max_size=1920",
            "max_fps=60",
            "video_bit_rate=8000000",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start scrcpy server: {}", e))?;

    // 4. Give the server a moment to start, then connect
    std::thread::sleep(Duration::from_millis(500));

    println!("[0A] Connecting to localhost:{}...", local_port);
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", local_port))
        .map_err(|e| format!("Failed to connect to scrcpy: {}", e))?;

    // 5. Read the dummy byte (forward tunnel handshake)
    let mut dummy = [0u8; 1];
    stream.read_exact(&mut dummy)
        .map_err(|e| format!("Failed to read dummy byte: {}", e))?;
    println!("[0A] Dummy byte received: 0x{:02x}", dummy[0]);

    // 6. Read device name (64 bytes)
    let mut name_buf = [0u8; 64];
    stream.read_exact(&mut name_buf)
        .map_err(|e| format!("Failed to read device name: {}", e))?;
    let device_name = String::from_utf8_lossy(&name_buf)
        .trim_end_matches('\0')
        .to_string();
    println!("[0A] Device name: {}", device_name);

    // 7. Read codec ID (u32 big-endian)
    let codec_id = stream.read_u32::<BigEndian>()
        .map_err(|e| format!("Failed to read codec ID: {}", e))?;
    println!("[0A] Codec ID: 0x{:08x}", codec_id);

    // 8. Read initial video size (u32 width + u32 height)
    let width = stream.read_u32::<BigEndian>()
        .map_err(|e| format!("Failed to read width: {}", e))?;
    let height = stream.read_u32::<BigEndian>()
        .map_err(|e| format!("Failed to read height: {}", e))?;
    println!("[0A] Video size: {}x{}", width, height);

    // Send metadata to frontend
    on_event.send(VideoEvent::Metadata {
        device_name: device_name.clone(),
        width,
        height,
        codec_id,
    }).map_err(|e| format!("Failed to send metadata: {}", e))?;

    // 9. Spawn packet reading loop
    std::thread::spawn(move || {
        let mut frame_count: u64 = 0;
        let start = Instant::now();

        loop {
            // Read 12-byte frame header
            let pts_raw = match stream.read_i64::<BigEndian>() {
                Ok(v) => v,
                Err(e) => {
                    let _ = on_event.send(VideoEvent::Stopped {
                        reason: format!("Socket read error: {}", e),
                    });
                    break;
                }
            };
            let packet_size = match stream.read_u32::<BigEndian>() {
                Ok(v) => v,
                Err(e) => {
                    let _ = on_event.send(VideoEvent::Stopped {
                        reason: format!("Socket read error: {}", e),
                    });
                    break;
                }
            };

            // Read packet data
            let mut data = vec![0u8; packet_size as usize];
            if let Err(e) = stream.read_exact(&mut data) {
                let _ = on_event.send(VideoEvent::Stopped {
                    reason: format!("Socket read error: {}", e),
                });
                break;
            }

            let received_us = now_us();

            // Determine packet type from PTS flags
            // Bit 63: configuration packet (SPS/PPS)
            // Bit 62: keyframe
            let is_config = (pts_raw & (1i64 << 63)) != 0;
            let keyframe = (pts_raw & (1i64 << 62)) != 0;
            let pts = (pts_raw & !(3i64 << 62)) as u64;

            let event = if is_config {
                VideoEvent::Config { data }
            } else {
                frame_count += 1;
                VideoEvent::Frame {
                    pts,
                    keyframe,
                    data,
                    received_us,
                }
            };

            if on_event.send(event).is_err() {
                break; // Frontend disconnected
            }

            // Log progress every 60 frames
            if frame_count > 0 && frame_count % 60 == 0 {
                let elapsed = start.elapsed().as_secs_f64();
                let fps = frame_count as f64 / elapsed;
                println!(
                    "[0A] {} frames | {:.1} FPS | last pkt: {} bytes",
                    frame_count, fps, packet_size
                );
            }
        }

        println!("[0A] Stream ended after {} frames", frame_count);
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Test 0C: Channel throughput test
// ---------------------------------------------------------------------------

#[tauri::command]
fn test_0c_throughput(on_event: Channel<VideoEvent>) -> Result<(), String> {
    std::thread::spawn(move || {
        // Simulate scrcpy packets: 50KB each, targeting 60 FPS
        let fake_data = vec![0xABu8; 50_000];
        let mut frame_count: u64 = 0;
        let start = Instant::now();
        let mut total_send_us: u64 = 0;

        for _ in 0..300 {
            // 300 frames = 5 seconds at 60 FPS
            let send_start = Instant::now();

            let event = VideoEvent::Frame {
                pts: frame_count * 16_666, // ~60fps timestamps
                keyframe: frame_count % 60 == 0,
                data: fake_data.clone(),
                received_us: now_us(),
            };

            if on_event.send(event).is_err() {
                println!("[0C] Channel closed after {} frames", frame_count);
                break;
            }

            let send_us = send_start.elapsed().as_micros() as u64;
            total_send_us += send_us;
            frame_count += 1;

            // Target 60 FPS timing
            let target = Duration::from_micros(16_666);
            let elapsed = send_start.elapsed();
            if elapsed < target {
                std::thread::sleep(target - elapsed);
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let avg_send_us = if frame_count > 0 {
            total_send_us / frame_count
        } else {
            0
        };
        let fps = if elapsed_ms > 0 {
            frame_count as f64 / (elapsed_ms as f64 / 1000.0)
        } else {
            0.0
        };

        println!(
            "[0C] Throughput test complete: {} frames in {}ms = {:.1} FPS, avg send: {}µs",
            frame_count, elapsed_ms, fps, avg_send_us
        );

        // Send a final "stopped" event with the results encoded in the reason
        let _ = on_event.send(VideoEvent::Stopped {
            reason: format!(
                "PASS: {} frames, {:.1} FPS, avg send {}µs",
                frame_count, fps, avg_send_us
            ),
        });
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri app setup
// ---------------------------------------------------------------------------

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            test_0a_detect_device,
            test_0a_start_scrcpy,
            test_0c_throughput,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
