//! Shell protocol v2 (`shell_protocol.h`): each frame is a 1-byte id, a u32 little-endian payload
//! length, then the payload. Ids 0 (stdin), 4 (close stdin) and 5 (window size) only travel towards
//! the device.

use crate::{Error, Result, ShellEvent};

const HEADER_LEN: usize = 5;
const ID_STDOUT: u8 = 1;
const ID_STDERR: u8 = 2;
const ID_EXIT: u8 = 3;

/// The most of one stdout or stderr frame returned as a single event. adbd sends frames of up to
/// its `MAX_PAYLOAD` (1 MiB); longer ones come out in pieces, as `ShellProtocol::Read` cuts them.
pub(crate) const CHUNK_MAX: usize = 64 * 1024;

/// Turns the bytes of a shell v2 stream into events, without I/O. The caller appends to `buf`.
#[derive(Debug, Default)]
pub(crate) struct Decoder {
    pub(crate) buf: Vec<u8>,
    /// The frame being decoded: its id and the payload bytes still to come.
    frame: Option<(u8, usize)>,
}

impl Decoder {
    /// The next event the buffered bytes complete, if any.
    pub(crate) fn decode(&mut self) -> Result<Option<ShellEvent>> {
        loop {
            let (id, remaining) = match self.frame {
                Some(frame) => frame,
                None => {
                    let Some(frame) = self.take_header()? else {
                        return Ok(None);
                    };
                    self.frame = Some(frame);
                    frame
                }
            };
            match id {
                ID_STDOUT | ID_STDERR => {
                    let n = remaining.min(CHUNK_MAX);
                    if self.buf.len() < n {
                        return Ok(None);
                    }
                    let data: Vec<u8> = self.buf.drain(..n).collect();
                    self.frame = (remaining > n).then_some((id, remaining - n));
                    return Ok(Some(if id == ID_STDOUT {
                        ShellEvent::Stdout(data)
                    } else {
                        ShellEvent::Stderr(data)
                    }));
                }
                ID_EXIT => {
                    let Some(&code) = self.buf.first() else {
                        return Ok(None);
                    };
                    self.buf.drain(..1);
                    self.frame = None;
                    return Ok(Some(ShellEvent::Exit(code)));
                }
                // AOSP's client ignores other ids too (`read_and_dump_protocol`).
                _ => {
                    let n = remaining.min(self.buf.len());
                    self.buf.drain(..n);
                    self.frame = (n < remaining).then_some((id, remaining - n));
                    if self.frame.is_some() {
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// True between frames with nothing buffered, the only place the stream may end.
    pub(crate) fn is_idle(&self) -> bool {
        self.frame.is_none() && self.buf.is_empty()
    }

    fn take_header(&mut self) -> Result<Option<(u8, usize)>> {
        let Some(&[id, l0, l1, l2, l3]) = self.buf.first_chunk::<HEADER_LEN>() else {
            return Ok(None);
        };
        let len = u32::from_le_bytes([l0, l1, l2, l3]);
        // adbd writes the exit code as a 1-byte payload (`Subprocess::WaitForExit`).
        if id == ID_EXIT && len != 1 {
            return Err(Error::Protocol(format!(
                "shell exit frame of {len} bytes, expected 1"
            )));
        }
        let len = usize::try_from(len)
            .map_err(|_| Error::Protocol(format!("shell frame of {len} bytes")))?;
        self.buf.drain(..HEADER_LEN);
        Ok(Some((id, len)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ShellEvent::{Exit, Stderr, Stdout};

    fn frame(id: u8, payload: &[u8]) -> Vec<u8> {
        let mut frame = vec![id];
        frame.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
        frame.extend_from_slice(payload);
        frame
    }

    fn decode_all(bytes: &[u8]) -> Result<Vec<ShellEvent>> {
        let mut decoder = Decoder::default();
        decoder.buf.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(event) = decoder.decode()? {
            events.push(event);
        }
        assert!(decoder.is_idle());
        Ok(events)
    }

    #[test]
    fn frames_become_events() {
        let bytes = [frame(1, b"hello\n"), frame(2, b"oops\n"), frame(3, &[42])].concat();
        assert_eq!(
            decode_all(&bytes).unwrap(),
            [
                Stdout(b"hello\n".to_vec()),
                Stderr(b"oops\n".to_vec()),
                Exit(42)
            ]
        );
    }

    #[test]
    fn header_is_id_then_little_endian_length() {
        let mut bytes = vec![2, 0x02, 0x01, 0, 0];
        bytes.extend_from_slice(&[b'e'; 0x0102]);
        assert_eq!(decode_all(&bytes).unwrap(), [Stderr(vec![b'e'; 258])]);
    }

    #[test]
    fn bytes_may_arrive_one_at_a_time() {
        let bytes = [frame(1, b"ab"), frame(2, b""), frame(3, &[0])].concat();
        let mut decoder = Decoder::default();
        let mut events = Vec::new();
        for &byte in &bytes {
            decoder.buf.push(byte);
            while let Some(event) = decoder.decode().unwrap() {
                events.push(event);
            }
        }
        assert_eq!(
            events,
            [Stdout(b"ab".to_vec()), Stderr(Vec::new()), Exit(0)]
        );
        assert!(decoder.is_idle());
    }

    #[test]
    fn frames_meant_for_the_device_are_skipped() {
        let bytes = [
            frame(5, b"24x80,0x0\0"),
            frame(0, b"stdin"),
            frame(4, b""),
            frame(9, &[1; 3000]),
            frame(1, b"after"),
        ]
        .concat();
        assert_eq!(decode_all(&bytes).unwrap(), [Stdout(b"after".to_vec())]);
    }

    #[test]
    fn long_frames_come_out_in_bounded_pieces() {
        let payload: Vec<u8> = (0..150 * 1024).map(|i| (i % 251) as u8).collect();
        let bytes = [frame(1, &payload), frame(3, &[1])].concat();
        let events = decode_all(&bytes).unwrap();
        let sizes: Vec<usize> = events
            .iter()
            .map(|event| match event {
                Stdout(data) => data.len(),
                _ => 0,
            })
            .collect();
        assert_eq!(sizes, [CHUNK_MAX, CHUNK_MAX, 150 * 1024 - 2 * CHUNK_MAX, 0]);
        let joined: Vec<u8> = events
            .iter()
            .filter_map(|event| match event {
                Stdout(data) => Some(data.as_slice()),
                _ => None,
            })
            .flatten()
            .copied()
            .collect();
        assert_eq!(joined, payload);
        assert_eq!(events.last(), Some(&Exit(1)));
    }

    #[test]
    fn a_huge_declared_length_allocates_nothing_up_front() {
        let mut decoder = Decoder::default();
        decoder
            .buf
            .extend_from_slice(&[1, 0xff, 0xff, 0xff, 0xff, b'x']);
        assert_eq!(decoder.decode().unwrap(), None);
        assert!(decoder.buf.capacity() < 1024);
        assert!(!decoder.is_idle());
    }

    #[test]
    fn exit_frame_must_hold_one_byte() {
        for bad in [frame(3, &[0, 0, 0, 0]), frame(3, b"")] {
            assert!(matches!(decode_all(&bad), Err(Error::Protocol(_))));
        }
    }

    #[test]
    fn a_partial_frame_is_not_idle() {
        let mut decoder = Decoder::default();
        decoder.buf.extend_from_slice(&[1, 4, 0]);
        assert_eq!(decoder.decode().unwrap(), None);
        assert!(!decoder.is_idle());
        decoder.buf.extend_from_slice(&[0, 0, b'a', b'b']);
        assert_eq!(decoder.decode().unwrap(), None);
        assert!(!decoder.is_idle());
        decoder.buf.extend_from_slice(b"cd");
        assert_eq!(decoder.decode().unwrap(), Some(Stdout(b"abcd".to_vec())));
        assert!(decoder.is_idle());
    }
}
