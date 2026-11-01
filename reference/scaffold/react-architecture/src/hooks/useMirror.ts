/**
 * useMirror — React hook managing the mirror session lifecycle.
 *
 * Connects to the Rust backend, receives video packets via Channel,
 * feeds them to the WebCodecs decoder, and tracks session state.
 *
 * Usage:
 *   const { start, stop, status, metadata, stats } = useMirror(canvasRef);
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { H264Decoder } from '../lib/decoder';
import * as ipc from '../lib/ipc';

export type MirrorStatus =
  | 'idle'
  | 'connecting'
  | 'configuring'  // received metadata, waiting for first frame
  | 'active'
  | 'stopped'
  | 'error';

export interface MirrorStats {
  framesReceived: number;
  framesDecoded: number;
  framesDropped: number;
  fps: number;
  lastPacketSize: number;
  queueDepth: number;
  elapsedSeconds: number;
}

interface UseMirrorReturn {
  start: (serial: string) => Promise<void>;
  stop: () => Promise<void>;
  status: MirrorStatus;
  error: string | null;
  metadata: ipc.StreamMetadata | null;
  stats: MirrorStats;
}

export function useMirror(
  canvasRef: React.RefObject<HTMLCanvasElement | null>,
): UseMirrorReturn {
  const [status, setStatus] = useState<MirrorStatus>('idle');
  const [error, setError] = useState<string | null>(null);
  const [metadata, setMetadata] = useState<ipc.StreamMetadata | null>(null);

  // Stats tracked in refs to avoid re-renders on every frame
  const statsRef = useRef<MirrorStats>({
    framesReceived: 0,
    framesDecoded: 0,
    framesDropped: 0,
    fps: 0,
    lastPacketSize: 0,
    queueDepth: 0,
    elapsedSeconds: 0,
  });
  const [stats, setStats] = useState<MirrorStats>(statsRef.current);

  const decoderRef = useRef<H264Decoder | null>(null);
  const startTimeRef = useRef(0);
  const activeRef = useRef(false);
  const statsIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Update displayed stats periodically (not every frame)
  const startStatsUpdater = useCallback(() => {
    if (statsIntervalRef.current) clearInterval(statsIntervalRef.current);
    statsIntervalRef.current = setInterval(() => {
      const s = statsRef.current;
      const elapsed = (Date.now() - startTimeRef.current) / 1000;
      s.elapsedSeconds = Math.round(elapsed);
      s.fps = elapsed > 0 ? Math.round((s.framesDecoded / elapsed) * 10) / 10 : 0;
      if (decoderRef.current) {
        s.queueDepth = decoderRef.current.queueDepth;
      }
      setStats({ ...s });
    }, 500);
  }, []);

  const stopStatsUpdater = useCallback(() => {
    if (statsIntervalRef.current) {
      clearInterval(statsIntervalRef.current);
      statsIntervalRef.current = null;
    }
  }, []);

  // Start a mirror session
  const start = useCallback(async (serial: string) => {
    const canvas = canvasRef.current;
    if (!canvas) {
      setError('Canvas not available');
      return;
    }

    // Reset state
    setStatus('connecting');
    setError(null);
    setMetadata(null);
    statsRef.current = {
      framesReceived: 0, framesDecoded: 0, framesDropped: 0,
      fps: 0, lastPacketSize: 0, queueDepth: 0, elapsedSeconds: 0,
    };
    activeRef.current = true;
    startTimeRef.current = Date.now();

    // Create the decoder
    const ctx = canvas.getContext('2d')!;
    const decoder = new H264Decoder({
      onFrame: (frame) => {
        if (!activeRef.current) {
          frame.close();
          return;
        }

        // Resize canvas to match frame
        if (canvas.width !== frame.displayWidth || canvas.height !== frame.displayHeight) {
          canvas.width = frame.displayWidth;
          canvas.height = frame.displayHeight;
        }

        // Render — this is a GPU-to-GPU blit, very fast
        ctx.drawImage(frame, 0, 0);

        // MUST close immediately to free GPU texture
        frame.close();

        statsRef.current.framesDecoded++;
      },
      onError: (err) => {
        console.error('[useMirror] decoder error:', err);
        setError(`Decoder error: ${err.message}`);
        setStatus('error');
      },
      onSizeChanged: (w, h) => {
        console.log(`[useMirror] size changed: ${w}x${h}`);
        canvas.width = w;
        canvas.height = h;
      },
    });
    decoderRef.current = decoder;

    startStatsUpdater();

    // Start the mirror and handle the Channel stream
    try {
      await ipc.startMirror(serial, (event) => {
        if (!activeRef.current) return;

        switch (event.type) {
          case 'metadata': {
            const meta: ipc.StreamMetadata = {
              device_name: event.device_name,
              width: event.width,
              height: event.height,
              codec_id: event.codec_id,
            };
            setMetadata(meta);
            canvas.width = event.width;
            canvas.height = event.height;

            // Configure decoder with initial dimensions
            decoder.configure(event.width, event.height);
            setStatus('configuring');
            break;
          }

          case 'config': {
            // Configuration packet (SPS/PPS).
            // With Annex B format, these are embedded in the stream.
            // The decoder was already configured with dimensions in metadata.
            // The SPS/PPS NALs in the config packet will be part of the
            // next keyframe, so we feed them as a keyframe.
            const configData = event.data instanceof ArrayBuffer
              ? new Uint8Array(event.data)
              : new Uint8Array(event.data);

            decoder.decode(configData, 0, true);
            break;
          }

          case 'frame': {
            if (status === 'configuring') {
              setStatus('active');
            }

            const frameData = event.data instanceof ArrayBuffer
              ? new Uint8Array(event.data)
              : new Uint8Array(event.data);

            statsRef.current.framesReceived++;
            statsRef.current.lastPacketSize = frameData.byteLength;

            const decoded = decoder.decode(frameData, event.pts, event.keyframe);
            if (!decoded) {
              statsRef.current.framesDropped++;
            }
            break;
          }

          case 'stopped': {
            setStatus('stopped');
            setError(event.reason);
            stopStatsUpdater();
            break;
          }
        }
      });
    } catch (e) {
      setStatus('error');
      setError(String(e));
      stopStatsUpdater();
    }
  }, [canvasRef, startStatsUpdater, stopStatsUpdater, status]);

  // Stop the mirror session
  const stop = useCallback(async () => {
    activeRef.current = false;
    stopStatsUpdater();

    try {
      await ipc.stopMirror();
    } catch {
      // Might already be stopped
    }

    if (decoderRef.current) {
      await decoderRef.current.close();
      decoderRef.current = null;
    }

    setStatus('idle');
  }, [stopStatsUpdater]);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      activeRef.current = false;
      stopStatsUpdater();
      decoderRef.current?.close();
    };
  }, [stopStatsUpdater]);

  return { start, stop, status, error, metadata, stats };
}
