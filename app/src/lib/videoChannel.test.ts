import { describe, expect, it } from "vitest";
import { FLAG_CONFIG, FLAG_KEY, parseDecoderConfig, parseVideoMessage } from "./videoChannel";

// The same vectors are in app/src-tauri/src/video_channel.rs.
const frameBytes = [1, 0, 0, 0, 0, 0, 0, 1, 2, 0, 0, 0, 0, 0, 0, 0x0a, 0x0b, 0xaa, 0xbb];
const configBytes = [
  2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0x01, 0xe0, 0, 0, 0x04, 0x10,
  ...new TextEncoder().encode("avc1.640028"),
];

const buffer = (bytes: number[]) => new Uint8Array(bytes).buffer;

describe("parseVideoMessage", () => {
  it("reads a key frame", () => {
    const message = parseVideoMessage(buffer(frameBytes));
    expect(message.flags & FLAG_KEY).toBe(FLAG_KEY);
    expect(message.ptsUs).toBe(0x0102);
    expect(message.recvUs).toBe(0x0a0b);
    expect([...message.payload]).toEqual([0xaa, 0xbb]);
  });

  it("reads a decoder-config record", () => {
    const message = parseVideoMessage(buffer(configBytes));
    expect(message.flags & FLAG_CONFIG).toBe(FLAG_CONFIG);
    expect(message.recvUs).toBe(7);
    expect(parseDecoderConfig(message.payload)).toEqual({ codec: "avc1.640028", width: 480, height: 1040 });
  });

  it("rejects truncated input", () => {
    expect(() => parseVideoMessage(buffer(frameBytes.slice(0, 16)))).toThrow(/too short/);
    expect(() => parseDecoderConfig(new Uint8Array(8))).toThrow(/too short/);
  });
});
