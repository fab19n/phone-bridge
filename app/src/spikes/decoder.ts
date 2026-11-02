import {
  type DecoderConfigRecord,
  FLAG_CONFIG,
  FLAG_KEY,
  nowUs,
  parseDecoderConfig,
  parseVideoMessage,
} from "../lib/videoChannel";
import { summarize } from "./stats";

/**
 * Decodes the video channel with WebCodecs and draws each frame to a canvas, keeping the stats the
 * spikes report. Follows the Stage 3 rules: reconfigure on every decoder-config record, then skip delta
 * frames until the next key frame.
 */
export class SpikeDecoder {
  private decoder: VideoDecoder | null = null;
  private readonly ctx: CanvasRenderingContext2D;
  private waitingForKey = true;
  /** performance.now() at decode(), by chunk timestamp. */
  private readonly submitted = new Map<number, number>();
  private readonly decodeMs: number[] = [];
  private readonly deliveryMs: number[] = [];
  private readonly configs: DecoderConfigRecord[] = [];
  /** Decoded frame sizes, one entry per change. */
  private readonly frameSizes: string[] = [];
  private readonly errors: string[] = [];
  messages = 0;
  private frames = 0;
  private decoded = 0;
  private skipped = 0;

  constructor(private readonly canvas: HTMLCanvasElement) {
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("no 2D canvas context");
    this.ctx = ctx;
  }

  handle(buffer: ArrayBuffer): void {
    this.messages++;
    const message = parseVideoMessage(buffer);
    this.deliveryMs.push((nowUs() - message.recvUs) / 1000);
    if (message.flags & FLAG_CONFIG) {
      this.configure(parseDecoderConfig(message.payload));
      return;
    }
    this.frames++;
    const key = (message.flags & FLAG_KEY) !== 0;
    if (this.decoder?.state !== "configured" || (this.waitingForKey && !key)) {
      this.skipped++;
      return;
    }
    this.waitingForKey = false;
    this.submitted.set(message.ptsUs, performance.now());
    try {
      this.decoder.decode(
        new EncodedVideoChunk({ type: key ? "key" : "delta", timestamp: message.ptsUs, data: message.payload }),
      );
    } catch (e) {
      this.errors.push(`decode: ${e}`);
    }
  }

  private configure(config: DecoderConfigRecord): void {
    if (!this.decoder || this.decoder.state === "closed") {
      this.decoder = new VideoDecoder({
        output: (frame) => this.draw(frame),
        error: (e) => this.errors.push(`decoder: ${e.message}`),
      });
    }
    this.decoder.configure({
      codec: config.codec,
      codedWidth: config.width,
      codedHeight: config.height,
      optimizeForLatency: true,
    });
    this.configs.push(config);
    this.waitingForKey = true;
  }

  private draw(frame: VideoFrame): void {
    const started = this.submitted.get(frame.timestamp);
    if (started !== undefined) {
      this.decodeMs.push(performance.now() - started);
      this.submitted.delete(frame.timestamp);
    }
    const size = `${frame.displayWidth}x${frame.displayHeight}`;
    if (this.frameSizes[this.frameSizes.length - 1] !== size) this.frameSizes.push(size);
    if (this.canvas.width !== frame.displayWidth || this.canvas.height !== frame.displayHeight) {
      this.canvas.width = frame.displayWidth;
      this.canvas.height = frame.displayHeight;
    }
    this.ctx.drawImage(frame, 0, 0);
    frame.close();
    this.decoded++;
  }

  async flush(): Promise<void> {
    if (this.decoder?.state === "configured") await this.decoder.flush();
  }

  close(): void {
    if (this.decoder && this.decoder.state !== "closed") this.decoder.close();
  }

  report() {
    return {
      messages: this.messages,
      frames: this.frames,
      decoded: this.decoded,
      skipped: this.skipped,
      errorCount: this.errors.length,
      errors: this.errors.slice(0, 10),
      configs: this.configs,
      frameSizes: this.frameSizes,
      canvas: `${this.canvas.width}x${this.canvas.height}`,
      decodeMs: summarize(this.decodeMs),
      deliveryMs: summarize(this.deliveryMs),
    };
  }
}
