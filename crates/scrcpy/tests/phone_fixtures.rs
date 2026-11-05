//! Replays the phone recordings in `fixtures/` (scrcpy v5.0.1 on a real device, `fixtures/README.md`)
//! through the public API. Each `<name>.json` comes from `scripts/describe-fixture.mjs`, a separate
//! JavaScript parser, so these tests compare two implementations of the format.

use phonebridge_scrcpy::h264::{self, NAL_TYPE_PPS, NAL_TYPE_SPS};
use phonebridge_scrcpy::video::{self, Codec, Header, PacketReader, VideoEvent, VideoStream};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

const NAL_TYPE_IDR: u8 = 5;

struct Fixture {
    name: &'static str,
    stream: &'static [u8],
    json: &'static str,
}

macro_rules! fixture {
    ($name:literal) => {
        Fixture {
            name: $name,
            stream: include_bytes!(concat!("../../../fixtures/", $name, ".bin")),
            json: include_str!(concat!("../../../fixtures/", $name, ".json")),
        }
    };
}

const FIXTURES: [Fixture; 3] = [
    fixture!("portrait-idle"),
    fixture!("rotation-midstream"),
    fixture!("high-motion"),
];

/// One capture session, as the JSON describes it.
#[derive(Debug, Default, PartialEq)]
struct Session {
    width: u32,
    height: u32,
    codec: String,
    configs: u64,
    frames: u64,
    key_frames: u64,
    first_pts_us: u64,
    last_pts_us: u64,
}

fn expected_sessions(fixture: &Fixture) -> Vec<Session> {
    let json: Value = serde_json::from_str(fixture.json).unwrap();
    let number = |session: &Value, key: &str| {
        session[key]
            .as_u64()
            .unwrap_or_else(|| panic!("{}: session {key} is missing or not a number", fixture.name))
    };
    let sessions: Vec<Session> = json["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| Session {
            width: number(s, "width") as u32,
            height: number(s, "height") as u32,
            codec: s["codec"]
                .as_str()
                .unwrap_or_else(|| panic!("{}: session codec is missing", fixture.name))
                .to_owned(),
            configs: number(s, "configs"),
            frames: number(s, "frames"),
            key_frames: number(s, "keyFrames"),
            first_pts_us: number(s, "firstPtsUs"),
            last_pts_us: number(s, "lastPtsUs"),
        })
        .collect();
    let frames: u64 = sessions.iter().map(|s| s.frames).sum();
    assert_eq!(
        json["totalFrames"].as_u64(),
        Some(frames),
        "{}",
        fixture.name
    );
    assert_eq!(
        json["totalBytes"].as_u64(),
        Some(fixture.stream.len() as u64),
        "{}",
        fixture.name
    );
    sessions
}

/// The same summary, from the crate's header parser.
async fn sessions_from_headers(fixture: &Fixture) -> Vec<Session> {
    let mut reader = fixture.stream;
    assert_eq!(video::read_codec(&mut reader).await.unwrap(), Codec::H264);
    let mut sessions: Vec<Session> = Vec::new();
    while let Some((header, payload)) = video::read_packet(&mut reader).await.unwrap() {
        match header {
            Header::Session {
                width,
                height,
                client_resized,
            } => {
                assert!(
                    !client_resized,
                    "{}: nothing resized the client",
                    fixture.name
                );
                sessions.push(Session {
                    width,
                    height,
                    ..Session::default()
                });
            }
            Header::Media {
                config,
                key,
                pts_us,
                ..
            } => {
                let session = sessions.last_mut().expect("a session packet comes first");
                if config {
                    session.configs += 1;
                    session.codec = h264::codec_string(&payload).unwrap();
                } else {
                    if session.frames == 0 {
                        session.first_pts_us = pts_us;
                    }
                    session.frames += 1;
                    session.key_frames += u64::from(key);
                    session.last_pts_us = pts_us;
                }
            }
        }
    }
    assert!(
        reader.is_empty(),
        "{}: the stream ends at a header boundary",
        fixture.name
    );
    sessions
}

async fn replay(stream: &[u8]) -> Vec<VideoEvent> {
    let mut reader = stream;
    let codec = video::read_codec(&mut reader).await.unwrap();
    let mut stream = VideoStream::new(codec);
    let mut events = Vec::new();
    while let Some((header, payload)) = video::read_packet(&mut reader).await.unwrap() {
        events.extend(stream.push(header, payload).unwrap());
    }
    events
}

fn nal_types(data: &[u8]) -> Vec<u8> {
    h264::nal_units(data)
        .into_iter()
        .filter_map(h264::nal_type)
        .collect()
}

#[tokio::test]
async fn headers_match_the_independent_parse() {
    for fixture in &FIXTURES {
        assert_eq!(
            sessions_from_headers(fixture).await,
            expected_sessions(fixture),
            "{}",
            fixture.name
        );
    }
}

#[tokio::test]
async fn events_follow_the_sessions() {
    for fixture in &FIXTURES {
        let name = fixture.name;
        let expected = expected_sessions(fixture);
        let events = replay(fixture.stream).await;
        let session_count = events
            .iter()
            .filter(|e| matches!(e, VideoEvent::Session { .. }))
            .count();
        assert_eq!(session_count, expected.len(), "{name}: sessions");

        // Session → DecoderConfig → key frame, once per session, with the session's size and the codec
        // string of the SPS in that key frame.
        let mut sessions = events
            .split(|e| matches!(e, VideoEvent::Session { .. }))
            .skip(1);
        let starts = events
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e, VideoEvent::Session { .. }));
        for ((at, start), session) in starts.zip(&expected) {
            assert_eq!(
                *start,
                VideoEvent::Session {
                    width: session.width,
                    height: session.height,
                },
                "{name}: event {at}"
            );
            let body = sessions.next().unwrap();
            let VideoEvent::DecoderConfig {
                codec,
                width,
                height,
            } = &body[0]
            else {
                panic!(
                    "{name}: event {}: a DecoderConfig follows each session",
                    at + 1
                );
            };
            assert_eq!(
                (codec.as_str(), *width, *height),
                (session.codec.as_str(), session.width, session.height),
                "{name}: event {}",
                at + 1
            );
            let VideoEvent::Frame {
                key: true, data, ..
            } = &body[1]
            else {
                panic!(
                    "{name}: event {}: a key frame follows each DecoderConfig",
                    at + 2
                );
            };
            assert_eq!(
                h264::codec_string(data).as_deref(),
                Some(codec.as_str()),
                "{name}"
            );

            let frames: Vec<(u64, bool)> = body[1..]
                .iter()
                .map(|e| match e {
                    VideoEvent::Frame { pts_us, key, .. } => (*pts_us, *key),
                    other => panic!("{name}: only frames after the first DecoderConfig: {other:?}"),
                })
                .collect();
            assert_eq!(frames.len() as u64, session.frames, "{name}");
            let keys = frames.iter().filter(|(_, key)| *key).count() as u64;
            assert_eq!(keys, session.key_frames, "{name}");
            assert_eq!(
                frames.first().map(|f| f.0),
                Some(session.first_pts_us),
                "{name}"
            );
            assert_eq!(
                frames.last().map(|f| f.0),
                Some(session.last_pts_us),
                "{name}"
            );
        }

        // The device clock only moves forward, across rotations too, in these recordings.
        let pts: Vec<u64> = events
            .iter()
            .filter_map(|e| match e {
                VideoEvent::Frame { pts_us, .. } => Some(*pts_us),
                _ => None,
            })
            .collect();
        assert!(
            pts.windows(2).all(|w| w[0] < w[1]),
            "{name}: PTS strictly increases"
        );
    }
}

#[tokio::test]
async fn key_frames_start_with_sps_and_pps_and_delta_frames_carry_no_config() {
    for fixture in &FIXTURES {
        for (i, event) in replay(fixture.stream).await.iter().enumerate() {
            let VideoEvent::Frame { key, data, .. } = event else {
                continue;
            };
            let types = nal_types(data);
            let is_config = |t: &u8| *t == NAL_TYPE_SPS || *t == NAL_TYPE_PPS;
            if *key {
                assert_eq!(
                    types[..2],
                    [NAL_TYPE_SPS, NAL_TYPE_PPS],
                    "{}: frame {i}",
                    fixture.name
                );
                assert!(
                    types[2..].contains(&NAL_TYPE_IDR) && !types[2..].iter().any(is_config),
                    "{}: frame {i}: one SPS/PPS, then IDR slices: {types:?}",
                    fixture.name
                );
            } else {
                assert!(
                    !types.is_empty() && !types.iter().any(|t| is_config(t) || *t == NAL_TYPE_IDR),
                    "{}: frame {i}: {types:?}",
                    fixture.name
                );
            }
        }
    }
}

/// A session loop: `select!` drops the pending read whenever its other branch wins. The 61-byte pipe
/// splits packets, headers included, at varying offsets, so most reads are dropped mid-packet.
#[tokio::test]
async fn packet_reader_survives_dropped_reads() {
    for fixture in &FIXTURES {
        let (mut tx, mut rx) = tokio::io::duplex(61);
        let bytes = fixture.stream;
        let writer = tokio::spawn(async move { tx.write_all(bytes).await });
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
            "{}: {dropped} dropped reads, {packets} packets",
            fixture.name
        );
        assert_eq!(events, replay(fixture.stream).await, "{}", fixture.name);
    }
}
