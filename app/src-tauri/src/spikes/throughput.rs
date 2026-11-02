//! Test 0C: can a raw-bytes Tauri Channel carry 60 frames of 50 KB per second, and how much worse is the
//! JSON path the spec proposed (REVIEW B1, B2)? Rust sends synthetic frames at a fixed rate from its own
//! thread; the page measures delivery latency against each message's `recv_us`.

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};

use crate::video_channel;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThroughputReport {
    mode: String,
    fps: u32,
    payload_bytes: u32,
    frames_sent: u32,
    elapsed_ms: u64,
    /// Time spent inside `Channel::send`, in µs (enqueue cost on the Rust side, not delivery).
    send_us_p50: u64,
    send_us_p95: u64,
    send_us_max: u64,
    /// Frames sent more than one period after their deadline (the sender couldn't keep up).
    late_frames: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonFrame<'a> {
    seq: u32,
    recv_us: u64,
    data: &'a [u8],
}

/// `mode` is `raw` (the video channel format) or `json` (bytes as a JSON number array, as in the spec).
#[tauri::command]
pub async fn spike_throughput(
    mode: String,
    fps: u32,
    payload_bytes: u32,
    seconds: u32,
    on_frame: Channel<InvokeResponseBody>,
) -> Result<ThroughputReport, String> {
    if !(1..=240).contains(&fps) || payload_bytes > 4 << 20 || !(1..=120).contains(&seconds) {
        return Err("fps, payload size or duration out of range".into());
    }
    let json = match mode.as_str() {
        "raw" => false,
        "json" => true,
        other => return Err(format!("unknown mode {other}")),
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    // A plain thread gives steadier pacing than the async runtime's timers.
    std::thread::spawn(move || {
        let _ = tx.send(send_frames(json, fps, payload_bytes, seconds, &on_frame));
    });
    rx.await.map_err(|e| e.to_string())?
}

fn send_frames(
    json: bool,
    fps: u32,
    payload_bytes: u32,
    seconds: u32,
    channel: &Channel<InvokeResponseBody>,
) -> Result<ThroughputReport, String> {
    // Deterministic bytes that aren't all zeros.
    let payload: Vec<u8> = (0..payload_bytes)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    let period = Duration::from_secs_f64(1.0 / f64::from(fps));
    let total = fps * seconds;
    let mut send_us = Vec::with_capacity(total as usize);
    let mut late_frames = 0;
    let start = Instant::now();

    for seq in 0..total {
        let deadline = start + period * seq;
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        } else if now - deadline > period {
            late_frames += 1;
        }
        let t0 = Instant::now();
        let recv_us = video_channel::now_us();
        let body = if json {
            let frame = JsonFrame {
                seq,
                recv_us,
                data: &payload,
            };
            InvokeResponseBody::Json(serde_json::to_string(&frame).map_err(|e| e.to_string())?)
        } else {
            // The PTS field carries the sequence number so the page can spot drops and reordering.
            InvokeResponseBody::Raw(video_channel::frame_message(
                seq % fps == 0,
                u64::from(seq),
                recv_us,
                &payload,
            ))
        };
        channel.send(body).map_err(|e| e.to_string())?;
        send_us.push(t0.elapsed().as_micros() as u64);
    }

    send_us.sort_unstable();
    let pick = |p: f64| send_us[((send_us.len() - 1) as f64 * p).round() as usize];
    Ok(ThroughputReport {
        mode: if json { "json" } else { "raw" }.into(),
        fps,
        payload_bytes,
        frames_sent: total,
        elapsed_ms: start.elapsed().as_millis() as u64,
        send_us_p50: pick(0.5),
        send_us_p95: pick(0.95),
        send_us_max: *send_us.last().unwrap_or(&0),
        late_frames,
    })
}
