//! The video channel from Rust to the frontend (DEVELOPMENT_PLAN Stage 2). Each message on the raw-bytes
//! Tauri Channel is a 17-byte big-endian header, `flags u8 | pts_us u64 | recv_us u64`, then a payload.
//!
//! A decoder-config record ([`FLAG_CONFIG`]) travels in-band, just before the key frame it configures,
//! because Tauri orders messages only within one Channel. Its payload is
//! `width u32 | height u32 | codec string (UTF-8)`. `recv_us` is when Rust received the data, in
//! microseconds since the Unix epoch, so the page can measure delivery latency.
//!
//! The TypeScript side is `app/src/lib/videoChannel.ts`; both test against the same golden bytes.

use std::time::{SystemTime, UNIX_EPOCH};

pub const HEADER_LEN: usize = 17;
/// The payload is a key frame.
pub const FLAG_KEY: u8 = 1 << 0;
/// The payload is a decoder-config record, not a frame.
pub const FLAG_CONFIG: u8 = 1 << 1;

/// Microseconds since the Unix epoch: the clock both sides use for `recv_us`.
pub fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}

pub fn frame_message(key: bool, pts_us: u64, recv_us: u64, data: &[u8]) -> Vec<u8> {
    message(if key { FLAG_KEY } else { 0 }, pts_us, recv_us, data)
}

pub fn decoder_config_message(codec: &str, width: u32, height: u32, recv_us: u64) -> Vec<u8> {
    let mut payload = Vec::with_capacity(8 + codec.len());
    payload.extend_from_slice(&width.to_be_bytes());
    payload.extend_from_slice(&height.to_be_bytes());
    payload.extend_from_slice(codec.as_bytes());
    message(FLAG_CONFIG, 0, recv_us, &payload)
}

fn message(flags: u8, pts_us: u64, recv_us: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.push(flags);
    out.extend_from_slice(&pts_us.to_be_bytes());
    out.extend_from_slice(&recv_us.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // The same vectors are in app/src/lib/videoChannel.test.ts.
    #[test]
    fn frame_golden_bytes() {
        assert_eq!(
            frame_message(true, 0x0102, 0x0A0B, &[0xAA, 0xBB]),
            [
                1, 0, 0, 0, 0, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0, 0x0A, 0x0B, 0xAA, 0xBB
            ]
        );
    }

    #[test]
    fn decoder_config_golden_bytes() {
        let mut expected = vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7];
        expected.extend_from_slice(&[0, 0, 0x01, 0xE0, 0, 0, 0x04, 0x10]);
        expected.extend_from_slice(b"avc1.640028");
        assert_eq!(
            decoder_config_message("avc1.640028", 480, 1040, 7),
            expected
        );
    }
}
