// Turns a fixture into an MP4, to check what a phone recording shows before it's committed: committed
// fixtures are as public as the repository. Writes fixtures/local/<name>.mp4 (git-ignored), and with --sheets
// also numbered contact sheets of every frame in fixtures/local/<name>-sheets/.
//
//   node scripts/preview-fixture.mjs fixtures/local/rotation-midstream.bin [--sheets]   (needs ffmpeg with libx264)
//
// Config and frame payloads are already Annex B, so their concatenation is a stream ffmpeg can read.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";
import { readFixture } from "./lib/scrcpy-fixture.mjs";

const input = process.argv[2];
if (!input) throw new Error("usage: node scripts/preview-fixture.mjs <fixture.bin> [--sheets]");
const { codecId, packets } = readFixture(readFileSync(input));
if (codecId !== "h264") throw new Error(`${input}: not an H.264 fixture`);
const payloads = packets.filter((p) => p.kind !== "session").map((p) => p.payload);
const sessions = packets.length - payloads.length;
const ffmpeg = (args) =>
  execFileSync("ffmpeg", ["-hide_banner", "-loglevel", "error", "-y", ...args], { stdio: "inherit" });

const name = basename(input, ".bin");
const localDir = fileURLToPath(new URL("../fixtures/local/", import.meta.url));
mkdirSync(localDir, { recursive: true });
const mp4 = join(localDir, `${name}.mp4`);
const dir = mkdtempSync(join(tmpdir(), "fixture-preview-"));
try {
  const stream = join(dir, "stream.h264");
  writeFileSync(stream, Buffer.concat(payloads));
  // A rotation changes the frame size mid-stream; scaling and padding into one box keeps a single output size.
  // The 960 px box shows a 480 px recording at twice its size, so small faces and text are visible when
  // checking it. The timing is nominal (30 fps): this is for looking at the content, not the frame rate.
  ffmpeg([
    "-framerate", "30", "-i", stream,
    "-vf", "scale=960:960:force_original_aspect_ratio=decrease,pad=960:960:(ow-iw)/2:(oh-ih)/2",
    "-c:v", "libx264", "-pix_fmt", "yuv420p", mp4,
  ]);
  console.log(`${mp4}: ${payloads.length} packets, ${sessions} session(s)`);

  if (process.argv.includes("--sheets")) {
    // Every frame at twice its size, 12 to a sheet, numbered in fixture order: something that shows for a frame
    // or two, like a notification, is easy to miss in the video. A rotation changes the frame size, so each
    // session gets its own sheets.
    const sheets = join(localDir, `${name}-sheets`);
    rmSync(sheets, { recursive: true, force: true });
    mkdirSync(sheets);
    const bySession = [];
    for (const p of packets) {
      if (p.kind === "session") bySession.push({ landscape: p.width > p.height, payloads: [], frames: 0 });
      else {
        bySession.at(-1).payloads.push(p.payload);
        if (p.kind === "frame") bySession.at(-1).frames++;
      }
    }
    // The numbers need a font file: Windows has Arial. Without one, the frames are still in order, row by row.
    // Each number goes in a strip under its frame, so it hides nothing.
    const font = "C:/Windows/Fonts/arial.ttf";
    let first = 1;
    for (const [i, session] of bySession.entries()) {
      if (!session.frames) {
        console.log(`session ${i + 1} has no frames: the take ended before its first frame`);
        continue;
      }
      const part = join(dir, `session-${i + 1}.h264`);
      writeFileSync(part, Buffer.concat(session.payloads));
      const label = existsSync(font)
        ? `pad=iw:ih+24:0:0:black,drawtext=fontfile='${font.replace(":", "\\:")}':` +
          `text='%{eif\\:n+${first}\\:d}':fontsize=20:fontcolor=yellow:x=4:y=h-th-3,`
        : "";
      const tile = session.landscape ? "3x4" : "6x2";
      const out = join(sheets, `session-${i + 1}-%02d.png`);
      ffmpeg(["-i", part, "-vf", `${label}scale=iw*2:ih*2,tile=${tile}:padding=6:color=red`, out]);
      first += session.frames;
    }
    console.log(`${sheets}: all ${first - 1} frames, 12 to a sheet${existsSync(font) ? ", numbered" : ""}`);
  }
} finally {
  rmSync(dir, { recursive: true, force: true });
}
