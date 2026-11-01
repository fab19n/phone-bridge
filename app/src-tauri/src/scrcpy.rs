//! scrcpy-server protocol client (Stage 2). Stage 0 holds only the pinned version.
//! Wire formats: protocol/video.md and protocol/control.md (both written against v5.0.1).

/// Exact scrcpy-server version this client is written against. It is passed as the first
/// server argument and must equal the JAR's `BuildConfig.VERSION_NAME`, or the server
/// exits with "The server version (..) does not match the client (..)".
/// Source: Genymobile/scrcpy tag v5.0.1, server/build.gradle (versionName "5.0.1")
/// and server/src/main/java/com/genymobile/scrcpy/Options.java (Options.parse).
pub const SERVER_VERSION: &str = "5.0.1";

/// SHA-256 of release asset `scrcpy-server-v5.0.1`, from the release's SHA256SUMS.txt.
/// `scripts/fetch-scrcpy-server.ps1` reads this and `SERVER_VERSION` from this file (keep the
/// `pub const NAME: &str = "...";` form) to download and verify the server. Stage 2 checks
/// the bundled file again before pushing it to a device.
pub const SERVER_SHA256: &str = "764eb6f79811d5211fe9df341120882ba9994c7a61b897d7bf3fb662e53bc536";

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
}
