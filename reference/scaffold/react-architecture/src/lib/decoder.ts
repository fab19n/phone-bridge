/**
 * WebCodecs H.264 decoder manager.
 *
 * Responsibilities:
 *  - Create and configure a VideoDecoder instance
 *  - Handle configuration packets (SPS/PPS) from scrcpy
 *  - Feed encoded H.264 frames to the decoder
 *  - Deliver decoded VideoFrame objects to a render callback
 *  - Manage decoder lifecycle (configure, reset on orientation change, close)
 *  - Track decode queue depth for latency control
 *
 * Technology: WebCodecs API (built into WebView2 / Chromium)
 * Input: Raw H.264 Annex B packets from scrcpy via Tauri Channel
 * Output: VideoFrame objects (GPU textures) ready for canvas rendering
 */

export interface DecoderCallbacks {
  onFrame: (frame: VideoFrame) => void;
  onError: (error: DOMException) => void;
  onSizeChanged?: (width: number, height: number) => void;
}

export class H264Decoder {
  private decoder: VideoDecoder | null = null;
  private callbacks: DecoderCallbacks;
  private configured = false;
  private currentWidth = 0;
  private currentHeight = 0;

  // Stats
  private framesDecoded = 0;
  private framesDropped = 0;

  constructor(callbacks: DecoderCallbacks) {
    this.callbacks = callbacks;
  }

  /**
   * Check if WebCodecs H.264 decoding is available.
   * Call this before creating a decoder instance.
   */
  static async isSupported(): Promise<boolean> {
    if (typeof VideoDecoder === 'undefined') return false;

    try {
      const result = await VideoDecoder.isConfigSupported({
        codec: 'avc1.640029', // High profile, level 4.1
        codedWidth: 1920,
        codedHeight: 1080,
      });
      return result.supported === true;
    } catch {
      return false;
    }
  }

  /**
   * Handle a configuration packet from scrcpy (contains SPS + PPS).
   * This (re)configures the decoder. Called on stream start and orientation changes.
   */
  configure(width: number, height: number): void {
    // Close existing decoder if reconfiguring (orientation change)
    if (this.decoder && this.decoder.state !== 'closed') {
      this.decoder.close();
    }

    this.currentWidth = width;
    this.currentHeight = height;

    this.decoder = new VideoDecoder({
      output: (frame: VideoFrame) => {
        // Check for size changes (orientation, fold/unfold)
        if (
          frame.displayWidth !== this.currentWidth ||
          frame.displayHeight !== this.currentHeight
        ) {
          this.currentWidth = frame.displayWidth;
          this.currentHeight = frame.displayHeight;
          this.callbacks.onSizeChanged?.(frame.displayWidth, frame.displayHeight);
        }

        this.framesDecoded++;
        this.callbacks.onFrame(frame);
        // IMPORTANT: the callback MUST call frame.close() after rendering.
        // If it doesn't, GPU memory leaks and the browser eventually crashes.
      },
      error: (err: DOMException) => {
        this.callbacks.onError(err);
      },
    });

    // Configure for H.264 Annex B stream.
    // No 'description' field — Annex B start codes are parsed by the decoder.
    // The first keyframe must contain SPS/PPS NALs for the decoder to produce output.
    this.decoder.configure({
      codec: 'avc1.640029',
      codedWidth: width,
      codedHeight: height,
      // optimizeForLatency: true — not yet widely supported but worth trying
    });

    this.configured = true;
  }

  /**
   * Feed an encoded H.264 frame to the decoder.
   * Returns false if the frame was dropped (queue too deep = latency risk).
   */
  decode(data: Uint8Array, pts: number, keyframe: boolean): boolean {
    if (!this.decoder || !this.configured || this.decoder.state !== 'configured') {
      return false;
    }

    // Latency control: if the decode queue is backing up, drop non-keyframes.
    // Each queued frame adds ~16ms of latency at 60 FPS.
    // Keeping the queue ≤3 bounds added latency to ~50ms.
    if (this.decoder.decodeQueueSize > 3) {
      if (!keyframe) {
        this.framesDropped++;
        return false;
      }
      // If it's a keyframe, we must decode it (decoder needs it to recover).
    }

    try {
      const chunk = new EncodedVideoChunk({
        type: keyframe ? 'key' : 'delta',
        timestamp: pts / 1000, // scrcpy sends nanoseconds, WebCodecs wants microseconds
        data: data,
      });
      this.decoder.decode(chunk);
      return true;
    } catch (e) {
      console.warn('[decoder] decode error:', e);
      return false;
    }
  }

  /**
   * Get the current decode queue depth.
   * Useful for the latency overlay.
   */
  get queueDepth(): number {
    return this.decoder?.decodeQueueSize ?? 0;
  }

  get stats() {
    return {
      framesDecoded: this.framesDecoded,
      framesDropped: this.framesDropped,
      queueDepth: this.queueDepth,
      configured: this.configured,
      state: this.decoder?.state ?? 'closed',
    };
  }

  /**
   * Flush pending frames and close the decoder.
   */
  async close(): Promise<void> {
    if (this.decoder && this.decoder.state !== 'closed') {
      try {
        await this.decoder.flush();
      } catch {
        // Flush can fail if decoder is in error state
      }
      this.decoder.close();
    }
    this.decoder = null;
    this.configured = false;
  }
}
