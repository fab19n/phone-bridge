/**
 * The video channel from Rust (app/src-tauri/src/video_channel.rs). Each message is a 17-byte
 * big-endian header, `flags u8 | ptsUs u64 | recvUs u64`, then a payload. A decoder-config record
 * (FLAG_CONFIG) carries `width u32 | height u32 | codec (UTF-8)` and arrives just before the key frame
 * it configures. `recvUs` is when Rust received the data, in µs since the Unix epoch.
 */
export const HEADER_LEN = 17;
/** The payload is a key frame. */
export const FLAG_KEY = 1 << 0;
/** The payload is a decoder-config record, not a frame. */
export const FLAG_CONFIG = 1 << 1;

export interface VideoMessage {
  flags: number;
  ptsUs: number;
  recvUs: number;
  payload: Uint8Array;
}

export interface DecoderConfigRecord {
  codec: string;
  width: number;
  height: number;
}

export function parseVideoMessage(buffer: ArrayBuffer): VideoMessage {
  if (buffer.byteLength < HEADER_LEN) {
    throw new Error(`video message too short: ${buffer.byteLength} bytes`);
  }
  const view = new DataView(buffer);
  return {
    flags: view.getUint8(0),
    ptsUs: Number(view.getBigUint64(1)),
    recvUs: Number(view.getBigUint64(9)),
    payload: new Uint8Array(buffer, HEADER_LEN),
  };
}

export function parseDecoderConfig(payload: Uint8Array): DecoderConfigRecord {
  if (payload.byteLength < 9) {
    throw new Error(`decoder-config record too short: ${payload.byteLength} bytes`);
  }
  const view = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
  return {
    width: view.getUint32(0),
    height: view.getUint32(4),
    codec: new TextDecoder().decode(payload.subarray(8)),
  };
}

/** Microseconds since the Unix epoch, on the same clock as `recvUs`. */
export function nowUs(): number {
  return (performance.timeOrigin + performance.now()) * 1000;
}
