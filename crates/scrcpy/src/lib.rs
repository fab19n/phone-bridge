//! The scrcpy-server v5.0.1 wire protocol: the server command line, the video stream, the control
//! socket, and H.264 codec strings for WebCodecs. Parsing is sans-IO where possible, so it unit-tests
//! without a device. Source of truth: `protocol/video.md` and `protocol/control.md`.

pub mod control;
pub mod h264;
pub mod server;
pub mod video;

/// Exact scrcpy-server version this client is written against. It is passed as the first
/// server argument and must equal the JAR's `BuildConfig.VERSION_NAME`, or the server
/// exits with "The server version (..) does not match the client (..)".
/// Source: Genymobile/scrcpy tag v5.0.1, server/build.gradle (versionName "5.0.1")
/// and server/src/main/java/com/genymobile/scrcpy/Options.java (Options.parse).
pub const SERVER_VERSION: &str = "5.0.1";

/// SHA-256 of release asset `scrcpy-server-v5.0.1`, from the release's SHA256SUMS.txt.
/// `scripts/fetch-scrcpy-server.ps1` reads this and `SERVER_VERSION` from this file (keep the
/// `pub const NAME: &str = "...";` form) to download and verify the server. [`is_pinned_server`]
/// checks the bundled file again before it is pushed to a device.
pub const SERVER_SHA256: &str = "764eb6f79811d5211fe9df341120882ba9994c7a61b897d7bf3fb662e53bc536";

/// Where the server is pushed on the device. With `cleanup=true` (the server default, which
/// [`server::ServerOptions::command`] keeps), the server's cleanup process deletes it right after
/// start-up, while the server is still running (`CleanUp.java` @ v5.0.1, `protocol/video.md` §2).
/// Push it before every launch, including a relaunch while another server instance is still alive.
pub const SERVER_DEVICE_PATH: &str = "/data/local/tmp/scrcpy-server.jar";

/// True if `jar` is byte-for-byte the pinned scrcpy-server release asset.
pub fn is_pinned_server(jar: &[u8]) -> bool {
    sha256_hex(jar) == SERVER_SHA256
}

/// Lowercase hex SHA-256, the format of [`SERVER_SHA256`].
fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;

    Sha256::digest(data)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_has_no_v_prefix() {
        // The server compares argv[0] to VERSION_NAME verbatim; "v5.0.1" would be rejected.
        assert!(!SERVER_VERSION.starts_with('v'));
    }

    #[test]
    fn sha256_is_64_lowercase_hex() {
        assert_eq!(SERVER_SHA256.len(), 64);
        assert!(
            SERVER_SHA256
                .chars()
                .all(|c| matches!(c, '0'..='9' | 'a'..='f'))
        );
    }

    #[test]
    fn sha256_hex_known_answers() {
        // FIPS 180-2 test vectors.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn other_files_are_not_the_pinned_server() {
        assert!(!is_pinned_server(b""));
        assert!(!is_pinned_server(b"PK\x03\x04 not the scrcpy server"));
        assert!(!is_pinned_server(SERVER_SHA256.as_bytes()));
    }
}
