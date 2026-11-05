// Writes fixtures/<name>.json, the values the replay tests expect, from fixtures/<name>.bin. It parses with
// scripts/lib/scrcpy-fixture.mjs, not the Rust crate, so the tests compare two implementations.
//
//   node scripts/describe-fixture.mjs fixtures/high-motion.bin [--description <text>] [--device <text>]
//
// --description and --device keep the values already in the JSON when omitted; a new fixture needs a
// description. Every other field is recomputed from the capture.
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { avcCodecString, nalType, nalUnits, NAL_SPS, readFixture } from "./lib/scrcpy-fixture.mjs";

const [input, ...rest] = process.argv.slice(2);
if (!input?.endsWith(".bin")) throw new Error("usage: node scripts/describe-fixture.mjs <fixture.bin> [...]");
const option = (name) => (rest.includes(name) ? rest[rest.indexOf(name) + 1] : undefined);
const output = input.replace(/\.bin$/, ".json");
const previous = existsSync(output) ? JSON.parse(readFileSync(output, "utf8")) : {};
const description = option("--description") ?? previous.description;
const device = option("--device") ?? previous.device;
if (!description) throw new Error(`${output} is new: pass --description`);

const bytes = readFileSync(input);
const { codecId, packets } = readFixture(bytes);
if (codecId !== "h264") throw new Error(`${input}: codec id ${JSON.stringify(codecId)}, expected "h264"`);

const sessions = [];
for (const packet of packets) {
  if (packet.kind === "session") {
    sessions.push({ width: packet.width, height: packet.height, codec: null, configs: 0, frames: 0, keyFrames: 0 });
    continue;
  }
  const session = sessions.at(-1);
  if (!session) throw new Error(`${input}: a media packet before the first session packet`);
  if (packet.kind === "config") {
    const sps = nalUnits(packet.payload).find((nal) => nalType(nal) === NAL_SPS);
    if (!sps) throw new Error(`${input}: a config packet without an SPS`);
    const codec = avcCodecString(sps);
    // One codec string per session is all the JSON can say; a mid-session change would need more.
    if (session.codec && session.codec !== codec) throw new Error(`${input}: codec changed within a session`);
    session.codec = codec;
    session.configs++;
    continue;
  }
  session.frames++;
  if (packet.key) session.keyFrames++;
  session.firstPtsUs ??= packet.ptsUs;
  session.lastPtsUs = packet.ptsUs;
}
// The replay tests need a config packet and at least one frame in every session. A take that stops just
// after a rotation can end with a session that has neither.
for (const [i, s] of sessions.entries()) {
  const which = `${input}: session ${i + 1} (${s.width}x${s.height})`;
  if (!s.frames) throw new Error(`${which} has no frames: the recording ended before its first frame; re-record it`);
  if (!s.codec) throw new Error(`${which} has frames but no config packet`);
}

const json = {
  describedBy: "scripts/describe-fixture.mjs",
  description,
  ...(device && { device }),
  codecId,
  sessions,
  totalFrames: sessions.reduce((n, s) => n + s.frames, 0),
  totalBytes: bytes.length,
};
writeFileSync(output, JSON.stringify(json, null, 2) + "\n");
console.log(`${output}: ${sessions.length} session(s), ${json.totalFrames} frames, ${json.totalBytes} bytes`);
