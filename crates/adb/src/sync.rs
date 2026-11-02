//! `sync:` file push (`SYNC.TXT`, now `docs/dev/sync.md`; `file_sync_protocol.h`; the SEND v1
//! path of `client/file_sync_client.cpp`). Every sync integer is little-endian.

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};

use crate::wire::read_exact;
use crate::{Error, Result};

/// The largest `DATA` chunk (`SYNC_DATA_MAX`).
pub(crate) const DATA_MAX: usize = 64 * 1024;
/// The longest `path,mode` adbd accepts (`handle_sync_command`, `daemon/file_sync_service.cpp`).
const PATH_MAX: usize = 1024;
/// The longest `FAIL` message accepted, the limit the AOSP client uses too.
const FAIL_MAX: u32 = 64 * 1024;

/// A sync request or response header: a 4-byte ASCII id and a u32 little-endian length.
pub(crate) fn header(id: &[u8; 4], len: u32) -> [u8; 8] {
    let mut header = [0; 8];
    header[..4].copy_from_slice(id);
    header[4..].copy_from_slice(&len.to_le_bytes());
    header
}

/// `SEND` with `<path>,<mode>`, the mode in decimal (`"%s,%d"`; adbd parses it with `strtoul`).
pub(crate) fn send_request(path: &str, mode: u32) -> Result<Vec<u8>> {
    let spec = format!("{path},{mode}");
    if spec.len() > PATH_MAX {
        return Err(Error::Protocol(format!(
            "sync path of {} bytes; adbd accepts at most {PATH_MAX}",
            spec.len()
        )));
    }
    let mut request = header(b"SEND", spec.len() as u32).to_vec();
    request.extend_from_slice(spec.as_bytes());
    Ok(request)
}

/// Writes `send` (from [`send_request`]), `data` as `DATA` chunks and `DONE` with `mtime`, reads
/// the device's verdict, then ends the sync session with `QUIT`.
pub(crate) async fn push<S>(stream: &mut S, send: &[u8], data: &[u8], mtime: u32) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let written = write_file(stream, send, data, mtime).await;
    let status = read_status(stream).await;
    let result = match (written, status) {
        (Ok(()), status) => status,
        // On failure adbd sends FAIL and may stop reading, so our writes fail: prefer its reason.
        (Err(_), Err(fail @ Error::Fail(_))) => Err(fail),
        (Err(error), _) => Err(error),
    };
    // After a FAIL adbd has already closed the session, so a failed QUIT changes nothing.
    if let Err(error) = stream.write_all(&header(b"QUIT", 0)).await {
        tracing::debug!(%error, "sync QUIT not sent");
    }
    result
}

async fn write_file<W: AsyncWrite + Unpin>(
    writer: &mut W,
    send: &[u8],
    data: &[u8],
    mtime: u32,
) -> Result<()> {
    writer.write_all(send).await?;
    let mut frame = Vec::with_capacity(8 + data.len().min(DATA_MAX));
    for chunk in data.chunks(DATA_MAX) {
        frame.clear();
        frame.extend_from_slice(&header(b"DATA", chunk.len() as u32));
        frame.extend_from_slice(chunk);
        writer.write_all(&frame).await?;
    }
    writer.write_all(&header(b"DONE", mtime)).await?;
    Ok(())
}

/// The reply to `DONE`: `OKAY` (its length "can be ignored", `sync.md`) or `FAIL` plus a message.
async fn read_status<R: AsyncRead + Unpin>(reader: &mut R) -> Result<()> {
    let mut status = [0; 8];
    read_exact(reader, &mut status, "the sync status").await?;
    let [a, b, c, d, l0, l1, l2, l3] = status;
    let len = u32::from_le_bytes([l0, l1, l2, l3]);
    match &[a, b, c, d] {
        b"OKAY" => Ok(()),
        b"FAIL" if len <= FAIL_MAX => {
            let mut message = vec![0; len as usize];
            read_exact(reader, &mut message, "the sync FAIL message").await?;
            Err(Error::Fail(String::from_utf8_lossy(&message).into_owned()))
        }
        b"FAIL" => Err(Error::Protocol(format!("sync FAIL message of {len} bytes"))),
        id => Err(Error::Protocol(format!(
            "unexpected sync response \"{}\"",
            id.escape_ascii()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_is_id_then_little_endian_length() {
        assert_eq!(header(b"DONE", 0x6543_2100), *b"DONE\x00\x21\x43\x65");
        assert_eq!(header(b"QUIT", 0), *b"QUIT\0\0\0\0");
    }

    #[test]
    fn send_carries_path_and_decimal_mode() {
        assert_eq!(
            send_request("/data/local/tmp/scrcpy-server.jar", 0o100644).unwrap(),
            b"SEND\x27\0\0\0/data/local/tmp/scrcpy-server.jar,33188"
        );
        assert_eq!(
            send_request("/sdcard/a", 0o644).unwrap(),
            b"SEND\x0d\0\0\0/sdcard/a,420"
        );
    }

    #[test]
    fn send_path_and_mode_fit_1024_bytes() {
        let longest = "/".repeat(PATH_MAX - ",420".len());
        assert_eq!(send_request(&longest, 0o644).unwrap().len(), 8 + PATH_MAX);
        let too_long = "/".repeat(PATH_MAX);
        assert!(matches!(
            send_request(&too_long, 0o644),
            Err(Error::Protocol(_))
        ));
    }

    /// Splits written sync frames back into `(id, length, payload)`; DONE carries no payload.
    fn frames(mut bytes: &[u8]) -> Vec<([u8; 4], u32, Vec<u8>)> {
        let mut frames = Vec::new();
        while let Some((head, rest)) = bytes.split_first_chunk::<8>() {
            let id = [head[0], head[1], head[2], head[3]];
            let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
            let payload_len = if &id == b"DONE" { 0 } else { len as usize };
            frames.push((id, len, rest[..payload_len].to_vec()));
            bytes = &rest[payload_len..];
        }
        assert!(bytes.is_empty());
        frames
    }

    #[tokio::test]
    async fn file_goes_out_as_64k_data_chunks_then_done() {
        let data: Vec<u8> = (0..150 * 1024).map(|i| (i % 251) as u8).collect();
        let send = send_request("/data/local/tmp/x", 0o100644).unwrap();
        let mut out = Vec::new();
        write_file(&mut out, &send, &data, 1_700_000_000)
            .await
            .unwrap();

        let frames = frames(&out);
        assert_eq!(
            frames[0],
            (*b"SEND", 23, b"/data/local/tmp/x,33188".to_vec())
        );
        let chunks: Vec<u32> = frames[1..frames.len() - 1]
            .iter()
            .map(|(id, len, _)| {
                assert_eq!(id, b"DATA");
                *len
            })
            .collect();
        assert_eq!(chunks, [65536, 65536, 22528]);
        let sent: Vec<u8> = frames[1..frames.len() - 1]
            .iter()
            .flat_map(|(_, _, payload)| payload.iter().copied())
            .collect();
        assert_eq!(sent, data);
        assert_eq!(
            frames.last().unwrap(),
            &(*b"DONE", 1_700_000_000, Vec::new())
        );
    }

    #[tokio::test]
    async fn empty_file_is_send_then_done() {
        let send = send_request("/data/local/tmp/empty", 0o644).unwrap();
        let mut out = Vec::new();
        write_file(&mut out, &send, &[], 7).await.unwrap();
        let ids: Vec<[u8; 4]> = frames(&out).into_iter().map(|(id, _, _)| id).collect();
        assert_eq!(ids, [*b"SEND", *b"DONE"]);
    }

    async fn status(mut bytes: &[u8]) -> Result<()> {
        read_status(&mut bytes).await
    }

    #[tokio::test]
    async fn status_after_done() {
        status(b"OKAY\0\0\0\0").await.unwrap();
        status(b"OKAY\x05\0\0\0").await.unwrap();
        let reason = "couldn't create file: Read-only file system";
        let reply = [&header(b"FAIL", reason.len() as u32)[..], reason.as_bytes()].concat();
        let fail = status(&reply).await;
        assert!(
            matches!(&fail, Err(Error::Fail(m)) if m == reason),
            "{fail:?}"
        );
        let cases: [&[u8]; 5] = [
            b"",
            b"OKAY\0\0",
            b"FAIL\x10\0\0\0short",
            b"FAIL\x01\0\x01\0",
            b"DATA\0\0\0\0",
        ];
        for bad in cases {
            assert!(
                matches!(status(bad).await, Err(Error::Protocol(_))),
                "{}",
                bad.escape_ascii()
            );
        }
    }
}
