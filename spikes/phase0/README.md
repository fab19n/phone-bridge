# Phase 0 harness (from the original scaffold)

Not buildable yet: it has no `package.json`, `Cargo.toml` or `tauri.conf.json`. Stage 1 makes it a real Tauri app.

**Known wrong before you run it:**
- It parses the **scrcpy v3** wire format. The pinned server is v5.0.1 (4-byte codec id, session packets,
  shifted PTS flags). See `docs/REVIEW.md` B16 and `protocol/video.md` §7–9.
- Test 0C times `send()`, not delivery, and sends JSON instead of raw bytes (REVIEW B1, B2).
- The config packet is discarded before decoding (B3); the server child process is never cleaned up (B9).

Stage 1 replaces these tests with the revised 0A–0H matrix in `docs/DEVELOPMENT_PLAN.md`.
