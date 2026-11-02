//! Replays `fixtures/synthetic-rotation.bin` (a v5.0.1 video stream with one rotation, written by
//! `scripts/make-synthetic-fixture.mjs`) through the public API and checks the events against
//! `fixtures/synthetic-rotation.json`.

use phonebridge_scrcpy::h264::{self, NAL_TYPE_PPS, NAL_TYPE_SPS};
use phonebridge_scrcpy::video::{self, Codec, PacketReader, VideoEvent, VideoStream};
use tokio::io::AsyncWriteExt;

const STREAM: &[u8] = include_bytes!("../../../fixtures/synthetic-rotation.bin");
const EXPECTED: &str = include_str!("../../../fixtures/synthetic-rotation.json");

/// Every `"key": value` in the generator's JSON, in document order, without quotes. The file is
/// small and machine-written, so this stands in for a JSON parser.
fn json_values(key: &str) -> Vec<&'static str> {
    let needle = format!("\"{key}\":");
    EXPECTED
        .match_indices(&needle)
        .map(|(at, _)| {
            let value = EXPECTED[at + needle.len()..].trim_start();
            let end = value.find([',', '\n', '}']).unwrap_or(value.len());
            value[..end].trim().trim_matches('"')
        })
        .collect()
}

fn json_numbers(key: &str) -> Vec<u64> {
    json_values(key)
        .iter()
        .map(|v| v.parse().unwrap())
        .collect()
}

fn nal_types(data: &[u8]) -> Vec<u8> {
    h264::nal_units(data)
        .into_iter()
        .filter_map(h264::nal_type)
        .collect()
}

async fn replay() -> Vec<VideoEvent> {
    let mut reader = STREAM;
    let codec = video::read_codec(&mut reader).await.unwrap();
    assert_eq!(codec, Codec::H264);
    let mut stream = VideoStream::new(codec);
    let mut events = Vec::new();
    while let Some((header, payload)) = video::read_packet(&mut reader).await.unwrap() {
        events.extend(stream.push(header, payload).unwrap());
    }
    assert!(reader.is_empty(), "the stream ends at a header boundary");
    events
}

/// A session loop: `select!` drops the pending read whenever its other branch wins. The 61-byte
/// pipe splits packets, headers included, at varying offsets, so most reads are dropped mid-packet.
#[tokio::test]
async fn packet_reader_survives_dropped_reads() {
    let (mut tx, mut rx) = tokio::io::duplex(61);
    let writer = tokio::spawn(async move { tx.write_all(STREAM).await });
    let codec = video::read_codec(&mut rx).await.unwrap();
    let mut reader = PacketReader::new(rx);
    let mut stream = VideoStream::new(codec);
    let mut events = Vec::new();
    let (mut packets, mut dropped) = (0, 0);
    loop {
        tokio::select! {
            biased;
            packet = reader.next() => {
                let Some((header, payload)) = packet.unwrap() else {
                    break;
                };
                packets += 1;
                events.extend(stream.push(header, payload).unwrap());
            }
            () = tokio::task::yield_now() => dropped += 1,
        }
    }
    writer.await.unwrap().unwrap();
    assert!(
        dropped > packets,
        "{dropped} dropped reads, {packets} packets"
    );
    assert_eq!(events, replay().await);
}

#[tokio::test]
async fn sessions_and_decoder_configs_match_the_fixture() {
    let events = replay().await;
    let sizes: Vec<(u32, u32)> = json_numbers("width")
        .into_iter()
        .zip(json_numbers("height"))
        .map(|(w, h)| (w as u32, h as u32))
        .collect();
    assert_eq!(sizes, [(480, 1040), (1040, 480)]);
    assert_eq!(json_numbers("totalBytes"), [STREAM.len() as u64]);

    let sessions: Vec<(u32, u32)> = events
        .iter()
        .filter_map(|e| match *e {
            VideoEvent::Session { width, height } => Some((width, height)),
            _ => None,
        })
        .collect();
    assert_eq!(sessions, sizes);

    let configs: Vec<(&str, u32, u32)> = events
        .iter()
        .filter_map(|e| match e {
            VideoEvent::DecoderConfig {
                codec,
                width,
                height,
            } => Some((codec.as_str(), *width, *height)),
            _ => None,
        })
        .collect();
    let expected: Vec<(&str, u32, u32)> = json_values("codec")
        .into_iter()
        .zip(&sizes)
        .map(|(codec, &(w, h))| (codec, w, h))
        .collect();
    assert_eq!(
        configs,
        [("avc1.640028", 480, 1040), ("avc1.640028", 1040, 480)]
    );
    assert_eq!(configs, expected);

    // Session → DecoderConfig → key frame, with the codec string of the SPS in that frame.
    for (i, event) in events.iter().enumerate() {
        if let VideoEvent::Session { .. } = event {
            assert!(matches!(events[i + 1], VideoEvent::DecoderConfig { .. }));
        }
        if let VideoEvent::DecoderConfig { codec, .. } = event {
            let VideoEvent::Frame {
                key: true, data, ..
            } = &events[i + 1]
            else {
                panic!("event {i}: a DecoderConfig must precede a key frame");
            };
            assert_eq!(h264::codec_string(data).as_deref(), Some(codec.as_str()));
        }
    }
}

#[tokio::test]
async fn frames_match_the_fixture() {
    let events = replay().await;
    let frames: Vec<(u64, bool, &[u8])> = events
        .iter()
        .filter_map(|e| match e {
            VideoEvent::Frame { pts_us, key, data } => Some((*pts_us, *key, data.as_slice())),
            _ => None,
        })
        .collect();
    assert_eq!(frames.len() as u64, json_numbers("totalFrames")[0]);
    assert_eq!(frames.len(), 120);
    let key_frames = frames.iter().filter(|(_, key, _)| *key).count() as u64;
    assert_eq!(key_frames, json_numbers("keyFrames").iter().sum::<u64>());
    assert_eq!(key_frames, 4);

    // Per session, in order.
    let per_session: Vec<(u64, u64)> = events
        .split(|e| matches!(e, VideoEvent::Session { .. }))
        .skip(1)
        .map(|session| {
            let frames = session
                .iter()
                .filter(|e| matches!(e, VideoEvent::Frame { .. }));
            let keys = frames
                .clone()
                .filter(|e| matches!(e, VideoEvent::Frame { key: true, .. }));
            (frames.count() as u64, keys.count() as u64)
        })
        .collect();
    let expected: Vec<(u64, u64)> = json_numbers("frames")
        .into_iter()
        .zip(json_numbers("keyFrames"))
        .collect();
    assert_eq!(per_session, expected);

    // The generator stamps frame i with round(i * 1e6 / fps) µs, so PTS strictly increases.
    let fps = json_numbers("fps")[0];
    for (i, (pts_us, _, _)) in frames.iter().enumerate() {
        assert_eq!(
            *pts_us,
            (2 * i as u64 * 1_000_000 + fps) / (2 * fps),
            "frame {i}"
        );
    }
    assert!(frames.windows(2).all(|w| w[0].0 < w[1].0));
}

#[tokio::test]
async fn every_key_frame_carries_the_session_config_and_delta_frames_none() {
    // The fixture, like an Android encoder, sends SPS/PPS once per session, so the second key frame
    // of each session gets them only from VideoStream.
    let events = replay().await;
    let mut key_frames = 0;
    for (i, event) in events.iter().enumerate() {
        let VideoEvent::Frame { key, data, .. } = event else {
            continue;
        };
        let types = nal_types(data);
        if *key {
            assert_eq!(types[..2], [NAL_TYPE_SPS, NAL_TYPE_PPS], "frame {i}");
            assert!(
                types[2..].iter().all(|&t| t == 5) && types.len() > 2,
                "frame {i}: IDR slices after SPS and PPS: {types:?}"
            );
            key_frames += 1;
        } else {
            assert!(
                !types.is_empty() && types.iter().all(|&t| t == 1),
                "frame {i}: {types:?}"
            );
        }
    }
    assert_eq!(key_frames, 4);
}
