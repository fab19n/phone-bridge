# Phase 0 harness from the original scaffold (reference only)

Superseded in Stage 1 by the real spike harness inside the app: the `spikes` cargo feature
(`app/src-tauri/src/spikes/`) and `app/src/spikes/`, run with `npm run tauri:spikes` or unattended with
`scripts/run-spikes.mjs`. Results: `docs/STAGE1_RESULTS.md`.

Kept to show what the review found. Don't build on it:
- It parses the **scrcpy v3** wire format. The pinned server is v5.0.1 (4-byte codec id, session packets,
  shifted PTS flags). See `docs/REVIEW.md` B16 and `protocol/video.md` §7–9.
- Test 0C times `send()`, not delivery, and sends JSON instead of raw bytes (REVIEW B1, B2).
- The config packet is discarded before decoding (B3); the server child process is never cleaned up (B9).
