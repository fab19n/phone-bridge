# Stream fixtures

Recorded raw scrcpy video-socket captures used for deterministic tests (DEVELOPMENT_PLAN Stage 1).
Each capture starts *after* the dummy byte and device meta: the 4-byte codec id, then 12-byte headers (session packets and
media packets with payloads) exactly as read from the video socket. Layout: `protocol/video.md` §7–9 (scrcpy v5.0.1).

`synthetic-rotation.bin` (with the expected values in `synthetic-rotation.json`) is not a phone recording.
`scripts/make-synthetic-fixture.mjs` builds it from libx264 output (ffmpeg with libx264 needed), framed exactly as
scrcpy v5.0.1 sends it, with one rotation. It stands in until the phone recordings exist.

Planned captures: `portrait-idle`, `rotation-midstream`, `high-motion`, recorded with the Stage 1 harness.
To add one: record it with `recordAs` (it lands in `fixtures/local/<name>.bin`, which is git-ignored), check that it
is well under 1 MB, move it to `fixtures/<name>.bin`, write `<name>.json` with its expected sessions, codecs and frame
counts, and add a replay test next to the synthetic one (`crates/scrcpy/tests/fixture_replay.rs`).
Fixtures that tests replay are **committed**, so CI runs without a phone. Keep each one small: record at a reduced size
and bitrate (e.g. `max_size=480`, `video_bit_rate=1000000`, 3–5 s), which stays well under 1 MB. Put large ad-hoc
captures in `fixtures/local/`, which is git-ignored.
