# Stream fixtures

Recorded raw scrcpy video-socket captures used for deterministic tests (DEVELOPMENT_PLAN Stage 1).
Each capture starts *after* the dummy byte and device meta: the 4-byte codec id, then 12-byte headers (session packets and
media packets with payloads) exactly as read from the video socket. Layout: `protocol/video.md` §7–9 (scrcpy v5.0.1).

Planned captures: `portrait-idle`, `rotation-midstream`, `high-motion`, recorded with the Stage 1 harness.
Fixtures that tests replay are **committed**, so CI runs without a phone. Keep each one small: record at a reduced size
and bitrate (e.g. `max_size=480`, `video_bit_rate=1000000`, 3–5 s), which stays well under 1 MB. Put large ad-hoc
captures in `fixtures/local/`, which is git-ignored.
