//! The video socket (`protocol/video.md` §5–11): device meta, codec id, then 12-byte headers that are
//! either session packets or media packets with a payload.

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::h264;

pub const DEVICE_NAME_LEN: usize = 64;
pub const HEADER_LEN: usize = 12;
/// Upper bound for one media packet, so a corrupt length can't allocate gigabytes.
pub const MAX_PACKET_LEN: u32 = 16 * 1024 * 1024;

// Codec ids (`protocol/video.md` §7) and header flags (§8–9, `Streamer.java` @ v5.0.1).
const CODEC_ID_H264: u32 = 0x6832_3634; // "h264"
const CODEC_ID_H265: u32 = 0x6832_3635; // "h265"
const CODEC_ID_AV1: u32 = 0x0061_7631; // "\0av1"
const SESSION_FLAGS: u32 = 0x8000_0000;
const FLAG_CONFIG: u64 = 1 << 62;
const FLAG_KEY_FRAME: u64 = 1 << 61;
const PTS_MASK: u64 = FLAG_KEY_FRAME - 1;

#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    #[error("video socket I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Codec id 0 on the video socket: the device must never disable video.
    #[error("the device disabled the video stream")]
    Disabled,
    /// Codec id 1: the server could not configure the encoder.
    #[error("video configuration error on the device")]
    ConfigError,
    #[error("unsupported codec id {0:#010x}")]
    UnsupportedCodec(u32),
    #[error("invalid session size {width}x{height}")]
    InvalidSessionSize { width: u32, height: u32 },
    #[error("invalid packet length {0}")]
    InvalidLength(u32),
    #[error("unexpected packet: {0}")]
    UnexpectedPacket(&'static str),
    /// [`PacketReader::next`] after an error: the stream can't be resynchronised.
    #[error("the video stream already failed")]
    AlreadyFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    H264,
    H265,
    Av1,
}

impl Codec {
    /// The 4-byte id that starts the stream (`protocol/video.md` §7).
    pub fn id(self) -> u32 {
        match self {
            Self::H264 => CODEC_ID_H264,
            Self::H265 => CODEC_ID_H265,
            Self::Av1 => CODEC_ID_AV1,
        }
    }
}

/// A parsed 12-byte header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Header {
    /// A new capture session: stream start, rotation, resize, `RESET_VIDEO` or an encoder retry.
    /// No payload follows.
    Session {
        width: u32,
        height: u32,
        client_resized: bool,
    },
    /// A media packet; `len` payload bytes follow. Config packets carry no PTS.
    Media {
        config: bool,
        key: bool,
        /// Microseconds on an opaque device clock (61 bits); 0 and meaningless when `config`.
        pts_us: u64,
        len: u32,
    },
}

impl Header {
    /// The 12 bytes that [`parse_header`] reads (`pts_us` keeps its low 61 bits), e.g. to record
    /// fixtures.
    pub fn to_bytes(self) -> [u8; HEADER_LEN] {
        let mut raw = [0; HEADER_LEN];
        match self {
            Self::Session {
                width,
                height,
                client_resized,
            } => {
                raw[..4]
                    .copy_from_slice(&(SESSION_FLAGS | u32::from(client_resized)).to_be_bytes());
                raw[4..8].copy_from_slice(&width.to_be_bytes());
                raw[8..].copy_from_slice(&height.to_be_bytes());
            }
            Self::Media {
                config,
                key,
                pts_us,
                len,
            } => {
                let mut pts_and_flags = pts_us & PTS_MASK;
                if config {
                    pts_and_flags |= FLAG_CONFIG;
                }
                if key {
                    pts_and_flags |= FLAG_KEY_FRAME;
                }
                raw[..8].copy_from_slice(&pts_and_flags.to_be_bytes());
                raw[8..].copy_from_slice(&len.to_be_bytes());
            }
        }
        raw
    }
}

/// What [`VideoStream`] produces for the frontend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VideoEvent {
    /// A new capture session started (also emitted for the first one).
    Session { width: u32, height: u32 },
    /// (Re)configure the decoder before the next frame: emitted after each session packet, and
    /// whenever a config packet changes the codec string. `width`/`height` come from the session packet.
    DecoderConfig {
        codec: String,
        width: u32,
        height: u32,
    },
    /// One access unit, with any pending config packet already merged in front (`protocol/video.md` §10).
    /// Every key frame also carries the session's SPS/PPS, so any key frame can start a decoder.
    Frame {
        pts_us: u64,
        key: bool,
        data: Vec<u8>,
    },
}

/// The 64-byte, NUL-padded UTF-8 device name: everything before the first NUL (all 64 bytes if
/// there is none), with invalid UTF-8 replaced.
pub fn parse_device_name(raw: &[u8; DEVICE_NAME_LEN]) -> String {
    let len = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..len]).into_owned()
}

/// The 4-byte big-endian codec id. 0 → [`VideoError::Disabled`], 1 → [`VideoError::ConfigError`].
pub fn parse_codec_id(raw: [u8; 4]) -> Result<Codec, VideoError> {
    match u32::from_be_bytes(raw) {
        0 => Err(VideoError::Disabled),
        1 => Err(VideoError::ConfigError),
        CODEC_ID_H264 => Ok(Codec::H264),
        CODEC_ID_H265 => Ok(Codec::H265),
        CODEC_ID_AV1 => Ok(Codec::Av1),
        id => Err(VideoError::UnsupportedCodec(id)),
    }
}

/// Parses a 12-byte header (`protocol/video.md` §8–9). A media length of 0 or above
/// [`MAX_PACKET_LEN`] is [`VideoError::InvalidLength`]; [`VideoStream`] checks session sizes.
pub fn parse_header(raw: &[u8; HEADER_LEN]) -> Result<Header, VideoError> {
    let [b0, b1, b2, b3, b4, b5, b6, b7, b8, b9, b10, b11] = *raw;
    if b0 & 0x80 != 0 {
        return Ok(Header::Session {
            width: u32::from_be_bytes([b4, b5, b6, b7]),
            height: u32::from_be_bytes([b8, b9, b10, b11]),
            client_resized: b3 & 1 != 0,
        });
    }
    let pts_and_flags = u64::from_be_bytes([b0, b1, b2, b3, b4, b5, b6, b7]);
    let len = u32::from_be_bytes([b8, b9, b10, b11]);
    if len == 0 || len > MAX_PACKET_LEN {
        return Err(VideoError::InvalidLength(len));
    }
    Ok(Header::Media {
        config: pts_and_flags & FLAG_CONFIG != 0,
        key: pts_and_flags & FLAG_KEY_FRAME != 0,
        pts_us: pts_and_flags & PTS_MASK,
        len,
    })
}

/// Turns packets into [`VideoEvent`]s: checks the packet order, merges config packets into the next
/// media packet, and decides when the decoder must be reconfigured.
///
/// Only H.264 has a codec string ([`h264::codec_string`]). With H.265 or AV1 the first frame fails
/// with [`VideoError::UnsupportedCodec`]; PhoneBridge always asks the server for H.264.
#[derive(Debug)]
pub struct VideoStream {
    codec: Codec,
    /// Width and height from the latest session packet; `None` before the first one.
    session: Option<(u32, u32)>,
    /// The current session's latest config packet (SPS + PPS for H.264).
    config: Option<Vec<u8>>,
    /// `config` arrived after the last media packet and still has to be merged into the next one.
    config_unmerged: bool,
    /// Set by each session packet: the next frame must be preceded by a `DecoderConfig`.
    needs_decoder_config: bool,
    last_codec: Option<String>,
}

impl VideoStream {
    pub fn new(codec: Codec) -> Self {
        Self {
            codec,
            session: None,
            config: None,
            config_unmerged: false,
            needs_decoder_config: false,
            last_codec: None,
        }
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    /// Feeds one packet (`payload` is empty for session packets) and returns the resulting events,
    /// in order.
    ///
    /// The first packet must be a session packet, and no session may be 0 pixels wide or high.
    /// A session packet drops any pending config, and its first frame needs a new one: the server
    /// always sends a config packet before it (`protocol/video.md` §11.2). A config packet is held,
    /// replacing any pending one, and merged in front of the next media packet (§10). A
    /// [`VideoEvent::DecoderConfig`] precedes that frame when the session is new or the codec
    /// string changed.
    ///
    /// Every later key frame of the session gets the session's config in front too, unless it
    /// already starts with an SPS. Android encoders send SPS/PPS once per session, and a decoder
    /// that is reconfigured mid-session (after a decode error, say) can only restart on a key
    /// frame that carries them. Delta frames are passed through unchanged.
    pub fn push(
        &mut self,
        header: Header,
        payload: Vec<u8>,
    ) -> Result<Vec<VideoEvent>, VideoError> {
        let (config, key, pts_us, len) = match header {
            Header::Session { width, height, .. } => {
                return self.start_session(width, height, &payload);
            }
            Header::Media {
                config,
                key,
                pts_us,
                len,
            } => (config, key, pts_us, len),
        };
        let Some((width, height)) = self.session else {
            return Err(VideoError::UnexpectedPacket(
                "media packet before the first session packet",
            ));
        };
        if len == 0 || usize::try_from(len).ok() != Some(payload.len()) {
            return Err(VideoError::InvalidLength(len));
        }
        if config {
            self.config = Some(payload);
            self.config_unmerged = true;
            return Ok(Vec::new());
        }

        let codec = match &self.config {
            Some(config) if self.config_unmerged => Some(codec_string(self.codec, config)?),
            _ if self.needs_decoder_config => {
                return Err(VideoError::UnexpectedPacket(
                    "frame without a config packet in a new session",
                ));
            }
            _ => None,
        };
        let mut events = Vec::with_capacity(2);
        if let Some(codec) = codec
            && (self.needs_decoder_config || self.last_codec.as_ref() != Some(&codec))
        {
            events.push(VideoEvent::DecoderConfig {
                codec: codec.clone(),
                width,
                height,
            });
            self.last_codec = Some(codec);
            self.needs_decoder_config = false;
        }
        let prepend = self.config_unmerged || (key && !starts_with_sps(&payload));
        self.config_unmerged = false;
        let data = match &self.config {
            Some(config) if prepend => [config.as_slice(), &payload].concat(),
            _ => payload,
        };
        events.push(VideoEvent::Frame { pts_us, key, data });
        Ok(events)
    }

    fn start_session(
        &mut self,
        width: u32,
        height: u32,
        payload: &[u8],
    ) -> Result<Vec<VideoEvent>, VideoError> {
        if !payload.is_empty() {
            return Err(VideoError::UnexpectedPacket(
                "session packet with a payload",
            ));
        }
        if width == 0 || height == 0 {
            return Err(VideoError::InvalidSessionSize { width, height });
        }
        self.session = Some((width, height));
        self.config = None;
        self.config_unmerged = false;
        self.needs_decoder_config = true;
        Ok(vec![VideoEvent::Session { width, height }])
    }
}

/// Whether an Annex-B payload's first NAL unit is an SPS (the encoder already put the parameter sets
/// in front of this key frame).
fn starts_with_sps(payload: &[u8]) -> bool {
    h264::nal_units(payload)
        .first()
        .is_some_and(|nal| h264::nal_type(nal) == Some(h264::NAL_TYPE_SPS))
}

/// The WebCodecs codec string for a config packet.
fn codec_string(codec: Codec, config: &[u8]) -> Result<String, VideoError> {
    match codec {
        Codec::H264 => h264::codec_string(config)
            .ok_or(VideoError::UnexpectedPacket("config packet without an SPS")),
        Codec::H265 | Codec::Av1 => Err(VideoError::UnsupportedCodec(codec.id())),
    }
}

/// Reads the 64-byte device meta.
///
/// # Cancel safety
///
/// Not cancel safe: a call that is dropped (in `select!`, or by a timeout) loses the bytes it
/// read, so stop using `reader` after one.
pub async fn read_device_name<R: AsyncRead + Unpin>(reader: &mut R) -> Result<String, VideoError> {
    let mut raw = [0; DEVICE_NAME_LEN];
    reader.read_exact(&mut raw).await?;
    Ok(parse_device_name(&raw))
}

/// Reads the 4-byte codec id. EOF here ([`VideoError::Io`] with `UnexpectedEof`) is how the
/// server reports a video encoder it could not configure; its stderr says why
/// (`protocol/video.md` §7).
///
/// # Cancel safety
///
/// Not cancel safe, like [`read_device_name`].
pub async fn read_codec<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Codec, VideoError> {
    let mut raw = [0; 4];
    reader.read_exact(&mut raw).await?;
    parse_codec_id(raw)
}

/// Reads one packet: its header and, for media packets, the payload. `Ok(None)` on a clean EOF at a
/// header boundary; EOF anywhere else is an error. After any error `reader` may be mid-packet, so
/// the stream is over.
///
/// # Cancel safety
///
/// Not cancel safe: a call that is dropped (in `select!`, or by a timeout) loses the bytes it
/// read, and the next call would parse from mid-packet. Stop reading `reader` after a dropped
/// call, or use [`PacketReader`], which keeps those bytes.
pub async fn read_packet<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Option<(Header, Vec<u8>)>, VideoError> {
    PacketReader::new(reader).next().await
}

/// Reads packets like [`read_packet`], but keeps a partly read packet between calls, so a session
/// can `select!` over [`PacketReader::next`] and its other work.
#[derive(Debug)]
pub struct PacketReader<R> {
    reader: R,
    state: ReadState,
}

#[derive(Debug)]
enum ReadState {
    Header {
        raw: [u8; HEADER_LEN],
        filled: usize,
    },
    Payload {
        header: Header,
        /// Allocated at the header's `len`, which `parse_header` bounds by [`MAX_PACKET_LEN`].
        payload: Vec<u8>,
        filled: usize,
    },
    Failed,
}

impl ReadState {
    const NEXT_PACKET: Self = Self::Header {
        raw: [0; HEADER_LEN],
        filled: 0,
    };
}

impl<R: AsyncRead + Unpin> PacketReader<R> {
    /// `reader` must be at a packet boundary: right after the codec id, or after a whole packet.
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            state: ReadState::NEXT_PACKET,
        }
    }

    /// The next packet, as [`read_packet`] returns it.
    ///
    /// # Cancel safety
    ///
    /// Cancel safe: bytes read by a call that is dropped (in `select!`, say) are kept for the next
    /// one. Any error ends the stream, and later calls return [`VideoError::AlreadyFailed`].
    pub async fn next(&mut self) -> Result<Option<(Header, Vec<u8>)>, VideoError> {
        let result = self.read().await;
        if result.is_err() {
            self.state = ReadState::Failed;
        }
        result
    }

    /// Cancel safe: each `.await` is a cancel-safe `read`, and its bytes go into `state` at once.
    async fn read(&mut self) -> Result<Option<(Header, Vec<u8>)>, VideoError> {
        let eof = || VideoError::Io(std::io::ErrorKind::UnexpectedEof.into());
        loop {
            match &mut self.state {
                ReadState::Header { raw, filled } => {
                    match self.reader.read(&mut raw[*filled..]).await? {
                        0 if *filled == 0 => return Ok(None),
                        0 => return Err(eof()),
                        n => *filled += n,
                    }
                    if *filled == HEADER_LEN {
                        let header = parse_header(raw)?;
                        let len = match header {
                            Header::Session { .. } => 0,
                            Header::Media { len, .. } => len as usize,
                        };
                        self.state = ReadState::Payload {
                            header,
                            payload: vec![0; len],
                            filled: 0,
                        };
                    }
                }
                ReadState::Payload {
                    header,
                    payload,
                    filled,
                } => {
                    if *filled == payload.len() {
                        let packet = (*header, std::mem::take(payload));
                        self.state = ReadState::NEXT_PACKET;
                        return Ok(Some(packet));
                    }
                    match self.reader.read(&mut payload[*filled..]).await? {
                        0 => return Err(eof()),
                        n => *filled += n,
                    }
                }
                ReadState::Failed => return Err(VideoError::AlreadyFailed),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::pin::{Pin, pin};
    use std::task::{Context, Poll, Waker};
    use tokio::io::ReadBuf;

    // Annex-B config packets: SPS + PPS with 4-byte start codes, as MediaCodec emits them.
    const CONFIG_HIGH_40: &[u8] = &[
        0, 0, 0, 1, 0x67, 0x64, 0x00, 0x28, 0xac, 0xb4, 0, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80,
    ];
    const CONFIG_BASELINE_31: &[u8] = &[
        0, 0, 0, 1, 0x67, 0x42, 0xe0, 0x1f, 0xda, 0, 0, 0, 1, 0x68, 0xce, 0x3c, 0x80,
    ];
    const IDR: &[u8] = &[0, 0, 0, 1, 0x65, 0x88, 0x84, 0x00, 0x33];
    const NON_IDR: &[u8] = &[0, 0, 0, 1, 0x41, 0x9a, 0x02, 0x04];

    fn session(width: u32, height: u32) -> (Header, Vec<u8>) {
        let header = Header::Session {
            width,
            height,
            client_resized: false,
        };
        (header, Vec::new())
    }

    fn media(config: bool, key: bool, pts_us: u64, payload: &[u8]) -> (Header, Vec<u8>) {
        let header = Header::Media {
            config,
            key,
            pts_us,
            len: payload.len() as u32,
        };
        (header, payload.to_vec())
    }

    fn config(payload: &[u8]) -> (Header, Vec<u8>) {
        media(true, false, 0, payload)
    }

    fn push(
        stream: &mut VideoStream,
        (header, payload): (Header, Vec<u8>),
    ) -> Result<Vec<VideoEvent>, VideoError> {
        stream.push(header, payload)
    }

    fn frame(pts_us: u64, key: bool, parts: &[&[u8]]) -> VideoEvent {
        VideoEvent::Frame {
            pts_us,
            key,
            data: parts.concat(),
        }
    }

    fn decoder_config(codec: &str, width: u32, height: u32) -> VideoEvent {
        VideoEvent::DecoderConfig {
            codec: codec.into(),
            width,
            height,
        }
    }

    /// A stream that has seen a session packet and the session's first frame.
    fn running_stream() -> VideoStream {
        let mut stream = VideoStream::new(Codec::H264);
        push(&mut stream, session(480, 1040)).unwrap();
        push(&mut stream, config(CONFIG_HIGH_40)).unwrap();
        push(&mut stream, media(false, true, 0, IDR)).unwrap();
        stream
    }

    #[test]
    fn device_name_is_cut_at_the_first_nul() {
        let mut raw = [0; DEVICE_NAME_LEN];
        raw[..7].copy_from_slice(b"Pixel 7");
        assert_eq!(parse_device_name(&raw), "Pixel 7");
        assert_eq!(parse_device_name(&[0; DEVICE_NAME_LEN]), "");

        let name = "Galaxy S23 – 日本".as_bytes();
        let mut raw = [0; DEVICE_NAME_LEN];
        raw[..name.len()].copy_from_slice(name);
        raw[name.len() + 1] = b'x'; // after the terminator: ignored
        assert_eq!(parse_device_name(&raw), "Galaxy S23 – 日本");
    }

    #[test]
    fn device_name_without_nul_or_with_invalid_utf8() {
        assert_eq!(parse_device_name(&[b'a'; DEVICE_NAME_LEN]), "a".repeat(64));
        let mut raw = [0; DEVICE_NAME_LEN];
        raw[..4].copy_from_slice(&[b'a', 0xff, 0xfe, b'b']);
        assert_eq!(parse_device_name(&raw), "a\u{fffd}\u{fffd}b");
    }

    #[test]
    fn codec_ids() {
        assert_eq!(parse_codec_id(*b"h264").unwrap(), Codec::H264);
        assert_eq!(parse_codec_id(*b"h265").unwrap(), Codec::H265);
        assert_eq!(parse_codec_id(*b"\0av1").unwrap(), Codec::Av1);
        for codec in [Codec::H264, Codec::H265, Codec::Av1] {
            assert_eq!(parse_codec_id(codec.id().to_be_bytes()).unwrap(), codec);
        }
        assert!(matches!(
            parse_codec_id([0, 0, 0, 0]),
            Err(VideoError::Disabled)
        ));
        assert!(matches!(
            parse_codec_id([0, 0, 0, 1]),
            Err(VideoError::ConfigError)
        ));
        for (raw, id) in [
            (*b"\0vp8", 0x0076_7038),
            (*b"\0vp9", 0x0076_7039),
            (*b"opus", 0x6f70_7573),
            ([0, 0, 0, 2], 2),
            ([0xff; 4], u32::MAX),
        ] {
            assert!(
                matches!(parse_codec_id(raw), Err(VideoError::UnsupportedCodec(got)) if got == id)
            );
        }
    }

    #[test]
    fn session_header() {
        let raw = [0x80, 0, 0, 0, 0, 0, 0x01, 0xe0, 0, 0, 0x04, 0x10];
        assert_eq!(
            parse_header(&raw).unwrap(),
            Header::Session {
                width: 480,
                height: 1040,
                client_resized: false
            }
        );
        let raw = [0x80, 0, 0, 0x01, 0, 0, 0x04, 0x38, 0, 0, 0x09, 0x60];
        assert_eq!(
            parse_header(&raw).unwrap(),
            Header::Session {
                width: 1080,
                height: 2400,
                client_resized: true
            }
        );
        // Sizes are VideoStream's to check, so a 0x0 session still parses.
        let raw = [0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(matches!(
            parse_header(&raw),
            Ok(Header::Session {
                width: 0,
                height: 0,
                ..
            })
        ));
    }

    #[test]
    fn media_header_flags_and_pts() {
        // Config: exactly bit 62, no PTS.
        let raw = [0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x25];
        assert_eq!(
            parse_header(&raw).unwrap(),
            Header::Media {
                config: true,
                key: false,
                pts_us: 0,
                len: 37
            }
        );
        // Key frame: bit 61, PTS 1_000_000 µs.
        let mut raw = [0; HEADER_LEN];
        raw[..8].copy_from_slice(&((1u64 << 61) | 1_000_000).to_be_bytes());
        raw[8..].copy_from_slice(&0x2d43u32.to_be_bytes());
        assert_eq!(
            parse_header(&raw).unwrap(),
            Header::Media {
                config: false,
                key: true,
                pts_us: 1_000_000,
                len: 0x2d43
            }
        );
        // The PTS is 61 bits: all of them are PTS, none is a flag.
        let mut raw = [0; HEADER_LEN];
        raw[..8].copy_from_slice(&PTS_MASK.to_be_bytes());
        raw[11] = 1;
        assert_eq!(
            parse_header(&raw).unwrap(),
            Header::Media {
                config: false,
                key: false,
                pts_us: (1 << 61) - 1,
                len: 1
            }
        );
    }

    #[test]
    fn media_length_is_bounded() {
        let with_len = |len: u32| {
            let mut raw = [0; HEADER_LEN];
            raw[8..].copy_from_slice(&len.to_be_bytes());
            parse_header(&raw)
        };
        assert!(matches!(with_len(0), Err(VideoError::InvalidLength(0))));
        assert!(matches!(with_len(1), Ok(Header::Media { len: 1, .. })));
        assert!(matches!(
            with_len(MAX_PACKET_LEN),
            Ok(Header::Media {
                len: MAX_PACKET_LEN,
                ..
            })
        ));
        let too_long = MAX_PACKET_LEN + 1;
        assert!(
            matches!(with_len(too_long), Err(VideoError::InvalidLength(len)) if len == too_long)
        );
        assert!(matches!(
            with_len(u32::MAX),
            Err(VideoError::InvalidLength(u32::MAX))
        ));
    }

    #[test]
    fn header_bytes_round_trip() {
        for header in [
            Header::Session {
                width: 1080,
                height: 2400,
                client_resized: false,
            },
            Header::Session {
                width: 1,
                height: 1,
                client_resized: true,
            },
            Header::Media {
                config: true,
                key: false,
                pts_us: 0,
                len: 37,
            },
            Header::Media {
                config: false,
                key: true,
                pts_us: 33_333,
                len: MAX_PACKET_LEN,
            },
            Header::Media {
                config: false,
                key: false,
                pts_us: PTS_MASK,
                len: 1,
            },
        ] {
            assert_eq!(parse_header(&header.to_bytes()).unwrap(), header);
        }
        let config = Header::Media {
            config: true,
            key: false,
            pts_us: 0,
            len: 37,
        };
        assert_eq!(
            config.to_bytes(),
            [0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x25],
            "as Streamer.java writes it"
        );
    }

    #[test]
    fn first_packet_must_be_a_session_packet() {
        let mut stream = VideoStream::new(Codec::H264);
        assert!(matches!(
            push(&mut stream, config(CONFIG_HIGH_40)),
            Err(VideoError::UnexpectedPacket(_))
        ));
        let mut stream = VideoStream::new(Codec::H264);
        assert!(matches!(
            push(&mut stream, media(false, true, 0, IDR)),
            Err(VideoError::UnexpectedPacket(_))
        ));
    }

    #[test]
    fn session_packet_emits_session() {
        let mut stream = VideoStream::new(Codec::H264);
        assert_eq!(
            push(&mut stream, session(480, 1040)).unwrap(),
            [VideoEvent::Session {
                width: 480,
                height: 1040
            }]
        );
    }

    #[test]
    fn zero_session_size_is_rejected_on_every_session_packet() {
        for (width, height) in [(0, 1040), (480, 0), (0, 0)] {
            let mut stream = VideoStream::new(Codec::H264);
            let Err(VideoError::InvalidSessionSize {
                width: w,
                height: h,
            }) = push(&mut stream, session(width, height))
            else {
                panic!("{width}x{height} accepted");
            };
            assert_eq!((w, h), (width, height));
            let mut stream = running_stream();
            assert!(matches!(
                push(&mut stream, session(width, height)),
                Err(VideoError::InvalidSessionSize { .. })
            ));
        }
    }

    #[test]
    fn config_packet_is_held_without_events() {
        let mut stream = VideoStream::new(Codec::H264);
        push(&mut stream, session(480, 1040)).unwrap();
        assert!(
            push(&mut stream, config(CONFIG_HIGH_40))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn first_frame_gets_decoder_config_and_merged_config() {
        let mut stream = VideoStream::new(Codec::H264);
        push(&mut stream, session(480, 1040)).unwrap();
        push(&mut stream, config(CONFIG_HIGH_40)).unwrap();
        assert_eq!(
            push(&mut stream, media(false, true, 0, IDR)).unwrap(),
            [
                decoder_config("avc1.640028", 480, 1040),
                frame(0, true, &[CONFIG_HIGH_40, IDR])
            ]
        );
    }

    #[test]
    fn config_is_merged_only_once_into_delta_frames() {
        let mut stream = running_stream();
        assert_eq!(
            push(&mut stream, media(false, false, 33_333, NON_IDR)).unwrap(),
            [frame(33_333, false, &[NON_IDR])]
        );
    }

    #[test]
    fn later_key_frames_carry_the_session_config() {
        let mut stream = running_stream();
        push(&mut stream, media(false, false, 33_333, NON_IDR)).unwrap();
        assert_eq!(
            push(&mut stream, media(false, true, 1_000_000, IDR)).unwrap(),
            [frame(1_000_000, true, &[CONFIG_HIGH_40, IDR])],
            "no DecoderConfig: same session, same codec"
        );
    }

    #[test]
    fn key_frame_that_already_starts_with_an_sps_is_left_alone() {
        let mut stream = running_stream();
        let inline = [CONFIG_HIGH_40, IDR].concat();
        assert_eq!(
            push(&mut stream, media(false, true, 1_000_000, &inline)).unwrap(),
            [frame(1_000_000, true, &[&inline])]
        );
    }

    #[test]
    fn later_config_replaces_the_pending_one() {
        let mut stream = VideoStream::new(Codec::H264);
        push(&mut stream, session(480, 1040)).unwrap();
        push(&mut stream, config(CONFIG_BASELINE_31)).unwrap();
        push(&mut stream, config(CONFIG_HIGH_40)).unwrap();
        assert_eq!(
            push(&mut stream, media(false, true, 0, IDR)).unwrap(),
            [
                decoder_config("avc1.640028", 480, 1040),
                frame(0, true, &[CONFIG_HIGH_40, IDR])
            ]
        );
    }

    #[test]
    fn session_packet_drops_the_pending_config() {
        let mut stream = VideoStream::new(Codec::H264);
        push(&mut stream, session(480, 1040)).unwrap();
        push(&mut stream, config(CONFIG_BASELINE_31)).unwrap();
        push(&mut stream, session(1040, 480)).unwrap();
        push(&mut stream, config(CONFIG_HIGH_40)).unwrap();
        assert_eq!(
            push(&mut stream, media(false, true, 0, IDR)).unwrap(),
            [
                decoder_config("avc1.640028", 1040, 480),
                frame(0, true, &[CONFIG_HIGH_40, IDR])
            ]
        );

        let mut stream = VideoStream::new(Codec::H264);
        push(&mut stream, session(480, 1040)).unwrap();
        push(&mut stream, config(CONFIG_HIGH_40)).unwrap();
        push(&mut stream, session(1040, 480)).unwrap();
        assert!(matches!(
            push(&mut stream, media(false, true, 0, IDR)),
            Err(VideoError::UnexpectedPacket(_))
        ));
    }

    #[test]
    fn every_session_reconfigures_the_decoder_even_with_the_same_codec() {
        let mut stream = running_stream();
        assert_eq!(
            push(&mut stream, session(1040, 480)).unwrap(),
            [VideoEvent::Session {
                width: 1040,
                height: 480
            }]
        );
        push(&mut stream, config(CONFIG_HIGH_40)).unwrap();
        assert_eq!(
            push(&mut stream, media(false, true, 66_667, IDR)).unwrap(),
            [
                decoder_config("avc1.640028", 1040, 480),
                frame(66_667, true, &[CONFIG_HIGH_40, IDR])
            ]
        );
    }

    #[test]
    fn mid_session_config_reconfigures_only_when_the_codec_changes() {
        let mut stream = running_stream();
        push(&mut stream, config(CONFIG_HIGH_40)).unwrap();
        assert_eq!(
            push(&mut stream, media(false, true, 33_333, IDR)).unwrap(),
            [frame(33_333, true, &[CONFIG_HIGH_40, IDR])]
        );
        push(&mut stream, config(CONFIG_BASELINE_31)).unwrap();
        assert_eq!(
            push(&mut stream, media(false, true, 66_667, IDR)).unwrap(),
            [
                decoder_config("avc1.42E01F", 480, 1040),
                frame(66_667, true, &[CONFIG_BASELINE_31, IDR])
            ]
        );
    }

    #[test]
    fn frame_without_config_in_a_new_session_is_rejected() {
        let mut stream = VideoStream::new(Codec::H264);
        push(&mut stream, session(480, 1040)).unwrap();
        assert!(matches!(
            push(&mut stream, media(false, true, 0, IDR)),
            Err(VideoError::UnexpectedPacket(_))
        ));
    }

    #[test]
    fn config_without_sps_is_rejected() {
        let mut stream = VideoStream::new(Codec::H264);
        push(&mut stream, session(480, 1040)).unwrap();
        push(&mut stream, config(&[0, 0, 0, 1, 0x68, 0xee, 0x3c, 0x80])).unwrap();
        assert!(matches!(
            push(&mut stream, media(false, true, 0, IDR)),
            Err(VideoError::UnexpectedPacket(_))
        ));
    }

    #[test]
    fn h265_and_av1_have_no_codec_string() {
        for codec in [Codec::H265, Codec::Av1] {
            let mut stream = VideoStream::new(codec);
            assert_eq!(stream.codec(), codec);
            push(&mut stream, session(480, 1040)).unwrap();
            assert!(
                push(&mut stream, config(&[0, 0, 0, 1, 0x40, 0x01]))
                    .unwrap()
                    .is_empty()
            );
            assert!(matches!(
                push(&mut stream, media(false, true, 0, &[0, 0, 0, 1, 0x26, 0x01])),
                Err(VideoError::UnsupportedCodec(id)) if id == codec.id()
            ));
        }
    }

    #[test]
    fn payload_must_match_the_header() {
        let mut stream = VideoStream::new(Codec::H264);
        let header = Header::Session {
            width: 480,
            height: 1040,
            client_resized: false,
        };
        assert!(matches!(
            stream.push(header, vec![1]),
            Err(VideoError::UnexpectedPacket(_))
        ));

        let mut stream = running_stream();
        let header = Header::Media {
            config: false,
            key: false,
            pts_us: 1,
            len: 5,
        };
        assert!(matches!(
            stream.push(header, vec![0; 4]),
            Err(VideoError::InvalidLength(5))
        ));
        let header = Header::Media {
            config: false,
            key: false,
            pts_us: 1,
            len: 0,
        };
        assert!(matches!(
            stream.push(header, Vec::new()),
            Err(VideoError::InvalidLength(0))
        ));
    }

    /// Hands out one byte per read, to exercise short reads.
    struct OneByteReader<'a>(&'a [u8]);

    impl AsyncRead for OneByteReader<'_> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if let Some((&byte, rest)) = self.0.split_first()
                && buf.remaining() > 0
            {
                buf.put_slice(&[byte]);
                self.0 = rest;
            }
            Poll::Ready(Ok(()))
        }
    }

    fn is_eof(result: Result<impl std::fmt::Debug, VideoError>) -> bool {
        matches!(result, Err(VideoError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof)
    }

    fn stream_bytes(packets: &[(Header, Vec<u8>)]) -> Vec<u8> {
        packets
            .iter()
            .flat_map(|(header, payload)| [header.to_bytes().to_vec(), payload.clone()])
            .flatten()
            .collect()
    }

    #[tokio::test]
    async fn reads_device_name_and_codec() {
        let mut bytes = [0; DEVICE_NAME_LEN].to_vec();
        bytes[..7].copy_from_slice(b"Pixel 7");
        bytes.extend_from_slice(b"h264");
        let mut reader = bytes.as_slice();
        assert_eq!(read_device_name(&mut reader).await.unwrap(), "Pixel 7");
        assert_eq!(read_codec(&mut reader).await.unwrap(), Codec::H264);

        assert!(is_eof(read_device_name(&mut [0u8; 63].as_slice()).await));
        assert!(is_eof(read_codec(&mut b"h26".as_slice()).await));
        assert!(is_eof(read_codec(&mut b"".as_slice()).await));
        assert!(matches!(
            read_codec(&mut [0u8; 4].as_slice()).await,
            Err(VideoError::Disabled)
        ));
    }

    #[tokio::test]
    async fn reads_packets_until_clean_eof() {
        let packets = [
            session(480, 1040),
            config(CONFIG_HIGH_40),
            media(false, true, 0, IDR),
            media(false, false, 33_333, NON_IDR),
        ];
        let bytes = stream_bytes(&packets);

        let mut reader = bytes.as_slice();
        for expected in &packets {
            assert_eq!(
                read_packet(&mut reader).await.unwrap().as_ref(),
                Some(expected)
            );
        }
        assert!(read_packet(&mut reader).await.unwrap().is_none());

        let mut reader = OneByteReader(&bytes);
        for expected in &packets {
            assert_eq!(
                read_packet(&mut reader).await.unwrap().as_ref(),
                Some(expected)
            );
        }
        assert!(read_packet(&mut reader).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn eof_inside_a_packet_is_an_error() {
        let bytes = stream_bytes(&[media(false, true, 0, IDR)]);
        for cut in 1..bytes.len() {
            assert!(
                is_eof(read_packet(&mut &bytes[..cut]).await),
                "cut at {cut}"
            );
            assert!(
                is_eof(read_packet(&mut OneByteReader(&bytes[..cut])).await),
                "cut at {cut}"
            );
        }
    }

    #[tokio::test]
    async fn invalid_lengths_are_rejected_before_reading_the_payload() {
        for len in [0, MAX_PACKET_LEN + 1, u32::MAX] {
            let mut bytes = [0u8; HEADER_LEN];
            bytes[8..].copy_from_slice(&len.to_be_bytes());
            let result = read_packet(&mut bytes.as_slice()).await;
            assert!(matches!(result, Err(VideoError::InvalidLength(got)) if got == len));
        }
    }

    /// A [`OneByteReader`] that is pending on every other poll, so a call that has read a byte is
    /// still running when [`poll_once`] drops it.
    struct StutteringReader<'a> {
        inner: OneByteReader<'a>,
        pending: bool,
    }

    impl AsyncRead for StutteringReader<'_> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            self.pending = !self.pending;
            if self.pending {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            Pin::new(&mut self.inner).poll_read(cx, buf)
        }
    }

    /// Polls `future` once and drops it, as `select!` does when another branch wins.
    fn poll_once<F: Future>(future: F) -> Poll<F::Output> {
        pin!(future).poll(&mut Context::from_waker(Waker::noop()))
    }

    #[test]
    fn packet_reader_keeps_what_dropped_calls_read() {
        let packets = [
            session(480, 1040),
            config(CONFIG_HIGH_40),
            media(false, true, 0, IDR),
            media(false, false, 33_333, NON_IDR),
            session(1040, 480),
        ];
        let bytes = stream_bytes(&packets);
        let mut reader = PacketReader::new(StutteringReader {
            inner: OneByteReader(&bytes),
            pending: false,
        });
        let mut got = Vec::new();
        loop {
            match poll_once(reader.next()) {
                Poll::Pending => {}
                Poll::Ready(Ok(Some(packet))) => got.push(packet),
                Poll::Ready(Ok(None)) => break,
                Poll::Ready(Err(e)) => panic!("{e}"),
            }
        }
        assert_eq!(got, packets);
    }

    #[tokio::test]
    async fn packet_reader_fails_for_good_after_an_error() {
        // The payload of a too-long packet is never read, so the bytes after its header must not
        // be parsed as the next packet.
        let too_long = Header::Media {
            config: false,
            key: false,
            pts_us: 0,
            len: MAX_PACKET_LEN + 1,
        };
        let mut bytes = too_long.to_bytes().to_vec();
        bytes.extend(stream_bytes(&[session(480, 1040)]));
        let mut reader = PacketReader::new(bytes.as_slice());
        assert!(matches!(
            reader.next().await,
            Err(VideoError::InvalidLength(_))
        ));
        for _ in 0..2 {
            assert!(matches!(
                reader.next().await,
                Err(VideoError::AlreadyFailed)
            ));
        }

        let bytes = stream_bytes(&[media(false, true, 0, IDR)]);
        let mut reader = PacketReader::new(&bytes[..bytes.len() - 1]);
        assert!(is_eof(reader.next().await));
        assert!(matches!(
            reader.next().await,
            Err(VideoError::AlreadyFailed)
        ));

        // A clean EOF is not an error.
        let mut reader = PacketReader::new(&b""[..]);
        assert!(reader.next().await.unwrap().is_none());
        assert!(reader.next().await.unwrap().is_none());
    }
}
