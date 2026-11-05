// Reads scrcpy v5.0.1 video-socket captures (fixtures/*.bin): the 4-byte codec id, then 12-byte headers
// (protocol/video.md §7–9). Used by preview-fixture.mjs and describe-fixture.mjs. It is written apart from
// the Rust parser in crates/scrcpy on purpose, so the fixture tests compare two implementations.

const SESSION = 1n << 63n;
const CONFIG = 1n << 62n;
const KEY_FRAME = 1n << 61n;
const PTS_MASK = KEY_FRAME - 1n;

/** The codec id and every packet in order. Throws if the capture ends inside a header or a payload. */
export function readFixture(bytes) {
  if (bytes.length < 4) throw new Error("no codec id");
  const codecId = bytes.subarray(0, 4).toString("latin1");
  const packets = [];
  for (let at = 4; at < bytes.length; ) {
    if (at + 12 > bytes.length) throw new Error(`truncated header at byte ${at}`);
    const head = bytes.readBigUInt64BE(at);
    if (head & SESSION) {
      packets.push({ kind: "session", width: bytes.readUInt32BE(at + 4), height: bytes.readUInt32BE(at + 8) });
      at += 12;
      continue;
    }
    const end = at + 12 + bytes.readUInt32BE(at + 8);
    if (end > bytes.length) throw new Error(`truncated packet at byte ${at}`);
    const payload = bytes.subarray(at + 12, end);
    if (head & CONFIG) {
      packets.push({ kind: "config", payload });
    } else {
      packets.push({ kind: "frame", key: (head & KEY_FRAME) !== 0n, ptsUs: Number(head & PTS_MASK), payload });
    }
    at = end;
  }
  return { codecId, packets };
}

/** The NAL units of an Annex B buffer (3- or 4-byte start codes), without their start codes. */
export function nalUnits(annexB) {
  const starts = [];
  for (let i = 0; i + 2 < annexB.length; i++) {
    if (annexB[i] === 0 && annexB[i + 1] === 0 && annexB[i + 2] === 1) {
      starts.push(i + 3);
      i += 2;
    }
  }
  return starts.map((start, n) => {
    let end = n + 1 < starts.length ? starts[n + 1] - 3 : annexB.length;
    // Zero bytes before a start code (a 4-byte start code's first byte, or trailing_zero_8bits) belong to no
    // unit; a unit's own last byte holds its stop bit, so it is never zero.
    while (end > start && annexB[end - 1] === 0) end--;
    return annexB.subarray(start, end);
  });
}

export const nalType = (nal) => nal[0] & 0x1f;
export const NAL_IDR = 5;
export const NAL_SPS = 7;
export const NAL_PPS = 8;

/** The WebCodecs codec string `avc1.PPCCLL` of an H.264 SPS NAL unit. */
export function avcCodecString(sps) {
  // After the NAL header: profile_idc, the constraint flags, level_idc. No emulation-prevention byte can
  // sit in these three: that needs two zero bytes before it, and the header and profile_idc are non-zero.
  if (sps.length < 4 || nalType(sps) !== NAL_SPS || sps[1] === 0) throw new Error("not an H.264 SPS");
  const hex = (byte) => byte.toString(16).padStart(2, "0").toUpperCase();
  return `avc1.${hex(sps[1])}${hex(sps[2])}${hex(sps[3])}`;
}
