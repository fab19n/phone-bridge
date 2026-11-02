// PhoneBridge Phase 0 — Validation Test Frontend
// Tests: WebCodecs support (0B), Channel throughput (0C), end-to-end mirror (0D)

import { useState, useRef, useEffect, useCallback } from 'react';
import { invoke, Channel } from '@tauri-apps/api/core';

// ---------------------------------------------------------------------------
// Types matching Rust VideoEvent
// ---------------------------------------------------------------------------

type VideoEvent =
  | { type: 'metadata'; device_name: string; width: number; height: number; codec_id: number }
  | { type: 'config'; data: number[] | ArrayBuffer }
  | { type: 'frame'; pts: number; keyframe: boolean; data: number[] | ArrayBuffer; received_us: number }
  | { type: 'stopped'; reason: string };

// ---------------------------------------------------------------------------
// Test 0B: WebCodecs support check
// ---------------------------------------------------------------------------

function Test0B() {
  const [result, setResult] = useState<string>('Not tested');
  const [details, setDetails] = useState<string[]>([]);

  async function runTest() {
    const log: string[] = [];

    // Check 1: VideoDecoder exists
    if (typeof VideoDecoder === 'undefined') {
      setResult('FAIL');
      log.push('VideoDecoder is not defined in this WebView');
      setDetails(log);
      return;
    }
    log.push('PASS: VideoDecoder class exists');

    // Check 2: H.264 baseline profile support
    try {
      const baseline = await VideoDecoder.isConfigSupported({
        codec: 'avc1.42001e', // Baseline profile, level 3.0
        codedWidth: 1920,
        codedHeight: 1080,
      });
      log.push(`H.264 Baseline: ${baseline.supported ? 'PASS' : 'FAIL'}`);
    } catch (e) {
      log.push(`H.264 Baseline: ERROR - ${e}`);
    }

    // Check 3: H.264 High profile support (what scrcpy typically outputs)
    try {
      const high = await VideoDecoder.isConfigSupported({
        codec: 'avc1.640029', // High profile, level 4.1
        codedWidth: 1920,
        codedHeight: 1080,
      });
      log.push(`H.264 High profile: ${high.supported ? 'PASS' : 'FAIL'}`);

      if (!high.supported) {
        setResult('FAIL');
        setDetails(log);
        return;
      }
    } catch (e) {
      log.push(`H.264 High profile: ERROR - ${e}`);
      setResult('FAIL');
      setDetails(log);
      return;
    }

    // Check 4: Can we create and configure a decoder?
    try {
      let frameReceived = false;
      const decoder = new VideoDecoder({
        output: (frame: VideoFrame) => {
          frameReceived = true;
          log.push(`PASS: Received decoded frame ${frame.displayWidth}x${frame.displayHeight}`);
          frame.close();
        },
        error: (err) => {
          log.push(`Decoder error: ${err.message}`);
        },
      });

      decoder.configure({
        codec: 'avc1.640029',
        codedWidth: 1920,
        codedHeight: 1080,
      });

      log.push(`PASS: Decoder configured, state=${decoder.state}`);
      decoder.close();
    } catch (e) {
      log.push(`Decoder creation: ERROR - ${e}`);
      setResult('FAIL');
      setDetails(log);
      return;
    }

    // Check 5: WebGL support (for VideoFrame rendering)
    try {
      const testCanvas = document.createElement('canvas');
      const gl = testCanvas.getContext('webgl2') || testCanvas.getContext('webgl');
      log.push(`WebGL: ${gl ? 'PASS' : 'FAIL (will use 2D canvas fallback)'}`);
    } catch (e) {
      log.push(`WebGL: ERROR - ${e}`);
    }

    setResult('PASS');
    setDetails(log);
  }

  return (
    <div style={{ border: '1px solid var(--border)', borderRadius: 8, padding: 16, marginBottom: 16 }}>
      <h3 style={{ margin: '0 0 8px' }}>Test 0B: WebCodecs in WebView2</h3>
      <button onClick={runTest} style={{ marginRight: 8, padding: '6px 16px' }}>
        Run test
      </button>
      <span style={{
        fontWeight: 500,
        color: result === 'PASS' ? 'green' : result === 'FAIL' ? 'red' : 'gray'
      }}>
        {result}
      </span>
      {details.length > 0 && (
        <pre style={{ marginTop: 8, fontSize: 12, whiteSpace: 'pre-wrap' }}>
          {details.join('\n')}
        </pre>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Test 0C: Channel throughput
// ---------------------------------------------------------------------------

function Test0C() {
  const [result, setResult] = useState<string>('Not tested');
  const [stats, setStats] = useState({ received: 0, fps: 0, avgSize: 0 });

  async function runTest() {
    setResult('Running...');

    let received = 0;
    const start = Date.now();
    let totalSize = 0;

    const channel = new Channel<VideoEvent>();
    channel.onmessage = (event: VideoEvent) => {
      if (event.type === 'frame') {
        received++;
        const data = event.data;
        totalSize += data instanceof ArrayBuffer ? data.byteLength : data.length;

        if (received % 30 === 0) {
          const elapsed = (Date.now() - start) / 1000;
          setStats({
            received,
            fps: Math.round(received / elapsed * 10) / 10,
            avgSize: Math.round(totalSize / received),
          });
        }
      } else if (event.type === 'stopped') {
        const elapsed = (Date.now() - start) / 1000;
        const fps = Math.round(received / elapsed * 10) / 10;
        setResult(fps >= 55 ? `PASS (${fps} FPS)` : `WARN (${fps} FPS — below 60)`);
        setStats({
          received,
          fps,
          avgSize: Math.round(totalSize / Math.max(received, 1)),
        });
      }
    };

    try {
      await invoke('test_0c_throughput', { onEvent: channel });
    } catch (e) {
      setResult(`ERROR: ${e}`);
    }
  }

  return (
    <div style={{ border: '1px solid var(--border)', borderRadius: 8, padding: 16, marginBottom: 16 }}>
      <h3 style={{ margin: '0 0 8px' }}>Test 0C: Channel throughput (50KB × 60 FPS)</h3>
      <button onClick={runTest} style={{ marginRight: 8, padding: '6px 16px' }}>
        Run test
      </button>
      <span style={{
        fontWeight: 500,
        color: result.startsWith('PASS') ? 'green' : result.startsWith('WARN') ? 'orange' : 'gray'
      }}>
        {result}
      </span>
      {stats.received > 0 && (
        <div style={{ marginTop: 8, fontSize: 13 }}>
          Received: {stats.received} frames | {stats.fps} FPS | avg packet: {stats.avgSize} bytes
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Test 0A + 0D: scrcpy connection + end-to-end mirror
// ---------------------------------------------------------------------------

function Test0AD() {
  const [devices, setDevices] = useState<string[]>([]);
  const [status, setStatus] = useState('Not started');
  const [metadata, setMetadata] = useState<{ name: string; w: number; h: number } | null>(null);
  const [frameStats, setFrameStats] = useState({ count: 0, fps: 0, lastSize: 0 });
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const decoderRef = useRef<VideoDecoder | null>(null);
  const startTimeRef = useRef<number>(0);
  const frameCountRef = useRef<number>(0);

  // Detect devices
  async function detectDevices() {
    try {
      const result = await invoke<string[]>('test_0a_detect_device');
      setDevices(result);
      setStatus(result.length > 0 ? `Found ${result.length} device(s)` : 'No devices found');
    } catch (e) {
      setStatus(`Error: ${e}`);
    }
  }

  // Configure WebCodecs decoder for the canvas
  const setupDecoder = useCallback((canvas: HTMLCanvasElement) => {
    if (decoderRef.current && decoderRef.current.state !== 'closed') {
      decoderRef.current.close();
    }

    const ctx = canvas.getContext('2d')!;

    const decoder = new VideoDecoder({
      output: (frame: VideoFrame) => {
        // Resize canvas if needed
        if (canvas.width !== frame.displayWidth || canvas.height !== frame.displayHeight) {
          canvas.width = frame.displayWidth;
          canvas.height = frame.displayHeight;
        }

        // Render
        ctx.drawImage(frame, 0, 0);
        frame.close();

        // Stats
        frameCountRef.current++;
        if (frameCountRef.current % 15 === 0) {
          const elapsed = (Date.now() - startTimeRef.current) / 1000;
          setFrameStats(prev => ({
            count: frameCountRef.current,
            fps: Math.round(frameCountRef.current / elapsed * 10) / 10,
            lastSize: prev.lastSize,
          }));
        }
      },
      error: (err) => {
        console.error('VideoDecoder error:', err);
        setStatus(`Decoder error: ${err.message}`);
      },
    });

    decoderRef.current = decoder;
    return decoder;
  }, []);

  // Start mirroring
  async function startMirror(serial: string) {
    if (!canvasRef.current) return;
    setStatus('Connecting...');

    const canvas = canvasRef.current;
    const decoder = setupDecoder(canvas);
    let configured = false;

    startTimeRef.current = Date.now();
    frameCountRef.current = 0;

    const channel = new Channel<VideoEvent>();
    channel.onmessage = (event: VideoEvent) => {
      switch (event.type) {
        case 'metadata':
          setMetadata({ name: event.device_name, w: event.width, h: event.height });
          canvas.width = event.width;
          canvas.height = event.height;
          setStatus(`Connected: ${event.device_name} (${event.width}x${event.height})`);
          break;

        case 'config':
          // SPS/PPS received — configure the decoder
          // For Annex B format, we don't pass a description to configure().
          // The SPS/PPS NALs will be in the first keyframe.
          if (decoder.state === 'unconfigured') {
            decoder.configure({
              codec: 'avc1.640029', // High profile, level 4.1
              codedWidth: metadata?.w || 1920,
              codedHeight: metadata?.h || 1080,
              // No 'description' for Annex B — start codes parsed automatically
            });
            configured = true;
            setStatus(prev => prev + ' — decoder configured');
          }

          // Feed config data as a keyframe chunk
          // The SPS/PPS needs to be part of the first keyframe
          // so the decoder has the parameter sets
          break;

        case 'frame': {
          if (!configured || decoder.state !== 'configured') return;

          // Drop frames if decoder queue is backing up (latency control)
          if (decoder.decodeQueueSize > 3) return;

          const rawData = event.data;
          const uint8 = rawData instanceof ArrayBuffer
            ? new Uint8Array(rawData)
            : new Uint8Array(rawData);

          try {
            const chunk = new EncodedVideoChunk({
              type: event.keyframe ? 'key' : 'delta',
              timestamp: Number(event.pts) / 1000, // ns → µs
              data: uint8,
            });
            decoder.decode(chunk);
          } catch (e) {
            console.warn('Decode error:', e);
          }

          setFrameStats(prev => ({
            ...prev,
            lastSize: uint8.byteLength,
          }));
          break;
        }

        case 'stopped':
          setStatus(`Stopped: ${event.reason}`);
          break;
      }
    };

    try {
      await invoke('test_0a_start_scrcpy', { serial, onEvent: channel });
    } catch (e) {
      setStatus(`Error: ${e}`);
    }
  }

  return (
    <div style={{ border: '1px solid var(--border)', borderRadius: 8, padding: 16, marginBottom: 16 }}>
      <h3 style={{ margin: '0 0 8px' }}>Test 0A + 0D: scrcpy stream + end-to-end mirror</h3>

      <div style={{ marginBottom: 8 }}>
        <button onClick={detectDevices} style={{ marginRight: 8, padding: '6px 16px' }}>
          Detect devices
        </button>
        {devices.map((serial) => (
          <button
            key={serial}
            onClick={() => startMirror(serial)}
            style={{ marginRight: 8, padding: '6px 16px' }}
          >
            Mirror {serial.substring(0, 8)}...
          </button>
        ))}
      </div>

      <div style={{ fontSize: 13, marginBottom: 8 }}>
        Status: {status}
      </div>

      {metadata && (
        <div style={{ fontSize: 13, marginBottom: 8 }}>
          Device: {metadata.name} | Resolution: {metadata.w}×{metadata.h} |
          Frames: {frameStats.count} | FPS: {frameStats.fps} |
          Last packet: {frameStats.lastSize} bytes
        </div>
      )}

      <canvas
        ref={canvasRef}
        width={360}
        height={640}
        style={{
          maxWidth: '100%',
          maxHeight: 500,
          border: '1px solid var(--border)',
          borderRadius: 8,
          backgroundColor: '#111',
          objectFit: 'contain',
        }}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main app
// ---------------------------------------------------------------------------

export default function App() {
  return (
    <div style={{ padding: 24, fontFamily: 'system-ui, sans-serif', maxWidth: 800 }}>
      <h1 style={{ fontSize: 24, margin: '0 0 4px' }}>PhoneBridge — Phase 0 validation</h1>
      <p style={{ color: 'gray', margin: '0 0 24px', fontSize: 14 }}>
        Run each test to validate the architecture before building the product.
      </p>

      <Test0B />
      <Test0C />
      <Test0AD />

      <div style={{ marginTop: 24, padding: 16, background: 'var(--surface-1, #f5f5f5)', borderRadius: 8 }}>
        <h3 style={{ margin: '0 0 8px', fontSize: 16 }}>Phase 0 checklist</h3>
        <ul style={{ margin: 0, paddingLeft: 20, fontSize: 14, lineHeight: 2 }}>
          <li>0A — scrcpy server connects and streams H.264 packets</li>
          <li>0B — WebCodecs VideoDecoder works in WebView2 (H.264 High profile)</li>
          <li>0C — Channel throughput ≥55 FPS at 50KB packets</li>
          <li>0D — Phone screen renders live in the canvas above</li>
        </ul>
      </div>
    </div>
  );
}
