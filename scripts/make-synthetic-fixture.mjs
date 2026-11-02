// Generates fixtures/synthetic-rotation.bin: a scrcpy v5.0.1 video stream, exactly as read from the video
// socket after the dummy byte and device meta, built from real H.264 that ffmpeg/libx264 encodes. The second
// half is rotated (new session packet, new config, new key frame), so tests cover the rotation path without a
// phone. Also writes fixtures/synthetic-rotation.json with the values tests should expect.
//
//   node scripts/make-synthetic-fixture.mjs        (needs ffmpeg with libx264 on PATH)
//
// Framing follows protocol/video.md §7–10 (v4.0+ layout): a 4-byte codec id, then 12-byte headers. A session
// packet has bit 63 set; a media header carries bit 62 = config, bit 61 = key frame and a 61-bit PTS in µs.
// Like Android's MediaCodec, the config packet holds only SPS + PPS and every NAL uses a 4-byte start code.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const FPS = 30;
const SEGMENTS = [
  { width: 480, height: 1040, seconds: 2 }, // portrait
  { width: 1040, height: 480, seconds: 2 }, // after rotation
];
const CODEC_ID_H264 = 0x68323634; // "h264"
const START_CODE = Buffer.from([0, 0, 0, 1]);

const repoRoot = fileURLToPath(new URL("..", import.meta.url));
const outBin = join(repoRoot, "fixtures", "synthetic-rotation.bin");
const outJson = join(repoRoot, "fixtures", "synthetic-rotation.json");

function encodeSegment({ width, height, seconds }, dir, index) {
  const out = join(dir, `segment-${index}.h264`);
  execFileSync("ffmpeg", [
    "-hide_banner", "-loglevel", "error", "-y",
    "-f", "lavfi", "-i", `testsrc2=size=${width}x${height}:rate=${FPS}`,
    "-t", String(seconds),
    "-c:v", "libx264", "-preset", "veryfast", "-tune", "zerolatency",
    "-profile:v", "high", "-level", "4.0", "-pix_fmt", "yuv420p",
    "-bf", "0", "-g", String(FPS), "-b:v", "1M", "-maxrate", "1M", "-bufsize", "1M",
    // AUDs mark access-unit boundaries for the split below; they are dropped from the packets.
    "-x264-params", "aud=1",
    "-f", "h264", out,
  ]);
  return readFileSync(out);
}

// Splits an Annex-B buffer into NAL units (without start codes).
function nalUnits(buf) {
  const starts = [];
  for (let i = 0; i + 3 <= buf.length; i++) {
    if (buf[i] === 0 && buf[i + 1] === 0 && buf[i + 2] === 1) {
      starts.push(i + 3);
      i += 2;
    }
  }
  return starts.map((start, n) => {
    let end = n + 1 < starts.length ? starts[n + 1] - 3 : buf.length;
    while (end > start && buf[end - 1] === 0) end--; // trailing zero of a 4-byte start code
    return buf.subarray(start, end);
  });
}

const nalType = (nal) => nal[0] & 0x1f;
const annexB = (nals) => Buffer.concat(nals.flatMap((nal) => [START_CODE, nal]));

function sessionHeader(width, height) {
  const h = Buffer.alloc(12);
  h.writeUInt32BE(0x80000000, 0); // bit 63 of the 64-bit header = session packet
  h.writeUInt32BE(width, 4);
  h.writeUInt32BE(height, 8);
  return h;
}

function mediaHeader({ config = false, key = false, ptsUs = 0n }, length) {
  let ptsAndFlags = config ? 1n << 62n : ptsUs;
  if (key) ptsAndFlags |= 1n << 61n;
  const h = Buffer.alloc(12);
  h.writeBigUInt64BE(ptsAndFlags, 0);
  h.writeUInt32BE(length, 8);
  return h;
}

const dir = mkdtempSync(join(tmpdir(), "pb-fixture-"));
const parts = [];
const codecId = Buffer.alloc(4);
codecId.writeUInt32BE(CODEC_ID_H264, 0);
parts.push(codecId);

const sessions = [];
let frameIndex = 0;
try {
  SEGMENTS.forEach((segment, index) => {
    const nals = nalUnits(encodeSegment(segment, dir, index));
    // Group into access units at each AUD (type 9).
    const accessUnits = [];
    for (const nal of nals) {
      if (nalType(nal) === 9 || accessUnits.length === 0) accessUnits.push([]);
      accessUnits[accessUnits.length - 1].push(nal);
    }

    parts.push(sessionHeader(segment.width, segment.height));
    let sps = null;
    let frames = 0;
    let keyFrames = 0;
    for (const au of accessUnits) {
      const configNals = au.filter((n) => nalType(n) === 7 || nalType(n) === 8); // SPS, PPS
      const sliceNals = au.filter((n) => [1, 5].includes(nalType(n))); // non-IDR, IDR slices
      // x264 repeats SPS/PPS before every IDR in a raw stream; MediaCodec emits its config buffer once per
      // session, so only the first one becomes a config packet and later repeats are dropped.
      if (configNals.length && !sps) {
        sps = configNals.find((n) => nalType(n) === 7);
        const data = annexB(configNals);
        parts.push(mediaHeader({ config: true }, data.length), data);
      }
      if (!sliceNals.length) continue;
      const key = sliceNals.some((n) => nalType(n) === 5);
      const data = annexB(sliceNals);
      const ptsUs = BigInt(Math.round((frameIndex * 1_000_000) / FPS));
      parts.push(mediaHeader({ key, ptsUs }, data.length), data);
      frames++;
      frameIndex++;
      if (key) keyFrames++;
    }
    if (!sps) throw new Error(`segment ${index}: no SPS found`);
    // avc1.PPCCLL from profile_idc, the constraint-flags byte and level_idc (SPS bytes 1–3).
    const hex = (b) => b.toString(16).padStart(2, "0").toUpperCase();
    const codec = `avc1.${hex(sps[1])}${hex(sps[2])}${hex(sps[3])}`;
    sessions.push({ width: segment.width, height: segment.height, codec, frames, keyFrames });
  });
} finally {
  rmSync(dir, { recursive: true, force: true });
}

const stream = Buffer.concat(parts);
writeFileSync(outBin, stream);
const expected = {
  generator: "scripts/make-synthetic-fixture.mjs",
  description: "scrcpy v5.0.1 video stream after the device meta: codec id, then session/config/media packets",
  codecId: "h264",
  fps: FPS,
  sessions,
  totalFrames: sessions.reduce((n, s) => n + s.frames, 0),
  totalBytes: stream.length,
};
writeFileSync(outJson, JSON.stringify(expected, null, 2) + "\n");
console.log(JSON.stringify(expected, null, 2));
