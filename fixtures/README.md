# Stream fixtures

Recorded raw scrcpy video-socket captures used for deterministic tests (DEVELOPMENT_PLAN Stage 1).
Each capture starts *after* the dummy byte and device meta: the 4-byte codec id, then 12-byte headers (session packets and
media packets with payloads), as read from the video socket. Layout: `protocol/video.md` §7–9 (scrcpy v5.0.1). The
recorder re-encodes each parsed header, which keeps every bit v5.0.1 defines.

Fixtures that tests replay are **committed**, so CI runs without a phone.

| Fixture | Source | What it covers | Test |
|---|---|---|---|
| `synthetic-rotation` | `scripts/make-synthetic-fixture.mjs` (libx264) | One rotation, two key frames per session, exact PTS | `crates/scrcpy/tests/fixture_replay.rs` |
| `portrait-idle` | Galaxy A56, Android 17 | A still screen: one frame, ten 100 ms repeats, then silence | `crates/scrcpy/tests/phone_fixtures.rs` |
| `rotation-midstream` | Galaxy A56, Android 17 | Portrait → landscape → portrait: three sessions, and a 1.35 s silence | `crates/scrcpy/tests/phone_fixtures.rs` |
| `high-motion` | Galaxy A56, Android 17 | Scrolling for the whole take, at the 60 fps and bitrate caps | `crates/scrcpy/tests/phone_fixtures.rs` |

Each `<name>.json` holds the values its test expects. For the phone fixtures, `scripts/describe-fixture.mjs` writes it
with its own JavaScript parser (`scripts/lib/scrcpy-fixture.mjs`), so the tests compare two implementations.

## Adding a phone fixture

1. Connect the phone with USB debugging on, and fetch the server (`scripts/fetch-scrcpy-server.ps1`).
2. **Keep personal content off the screen**: a fixture shows the phone's screen, and it's as public as the
   repository, history included. Open a screen without photos, faces, account pictures, names or documents: a
   settings page without your account on it (on Samsung, a sub-page such as Display, not the main page) works for
   all three recordings. Clear your notifications and turn on Do Not Disturb, so no notification or its icon shows.
3. Record: `node scripts/run-spikes.mjs --build --record [--only <name>,...]` writes `fixtures/local/<name>.bin`
   (git-ignored) at 480 px, 60 fps and 1 Mbps. `--build` makes the release build with the `spikes` feature; leave it
   out when re-recording with the same build. A banner in the app window says when to rotate the phone or move the
   screen, and says when a take came out wrong (wrong orientation, a missing or unwanted rotation, too little or too
   much change, or no usable video). The run then exits with code 2, or with 1 if a take failed outright (and then
   doesn't write `--out`). To add a new kind of recording, add it to `RECORDINGS` in that script, with a `check`
   for what a usable take has.
4. **Check every frame before committing it**: `node scripts/preview-fixture.mjs fixtures/local/<name>.bin --sheets`
   (needs ffmpeg with libx264) writes `fixtures/local/<name>.mp4` at twice the recorded size, and numbered contact
   sheets of every frame in `fixtures/local/<name>-sheets/`. Look through all the sheets, not just the video:
   something that shows for a frame or two is easy to miss.
5. Move it to `fixtures/<name>.bin` (keep each one well under 1 MB), then write its JSON:
   `node scripts/describe-fixture.mjs fixtures/<name>.bin --description "..." --device "..."`.
6. Add it to `FIXTURES` in `crates/scrcpy/tests/phone_fixtures.rs`, and run `cargo test -p phonebridge-scrcpy`.

Put large ad-hoc captures in `fixtures/local/`, which is git-ignored.
