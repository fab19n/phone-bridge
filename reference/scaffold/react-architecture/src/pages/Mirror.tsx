/**
 * Mirror page — the main mirroring view.
 *
 * Assembles MirrorCanvas + ControlBar + LatencyOverlay into a complete
 * mirror session interface. Manages the session lifecycle and handles
 * keyboard shortcuts.
 */

import { useRef, useCallback, useEffect, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { useMirror } from '../hooks/useMirror';
import { MirrorCanvas } from '../components/mirror/MirrorCanvas';
import { ControlBar } from '../components/mirror/ControlBar';
import * as ipc from '../lib/ipc';

export function MirrorPage() {
  const { serial } = useParams<{ serial: string }>();
  const navigate = useNavigate();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const { start, stop, status, error, metadata, stats } = useMirror(canvasRef);
  const [showLatency, setShowLatency] = useState(false);

  // Start mirroring when the page mounts
  useEffect(() => {
    if (!serial) {
      navigate('/');
      return;
    }

    start(serial).catch((e) => {
      console.error('Failed to start mirror:', e);
    });

    return () => {
      stop();
    };
    // Only run on mount/unmount — serial won't change mid-session
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serial]);

  // Screenshot handler
  const handleScreenshot = useCallback(async () => {
    try {
      const path = await ipc.takeScreenshot();
      console.log('Screenshot saved:', path);
      // Could show a toast notification here
    } catch (e) {
      console.error('Screenshot failed:', e);
    }
  }, []);

  // Global keyboard shortcut: Ctrl+L toggles latency overlay
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.key === 'l') {
        e.preventDefault();
        setShowLatency((v) => !v);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  // Device size for coordinate translation
  const deviceSize = metadata
    ? { width: metadata.width, height: metadata.height }
    : null;

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        height: '100vh',
        overflow: 'hidden',
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '8px 16px',
          borderBottom: '1px solid var(--border, #eee)',
          flexShrink: 0,
        }}
      >
        <button
          onClick={() => { stop(); navigate('/'); }}
          style={{
            padding: '6px 12px',
            border: '1px solid var(--border, #ddd)',
            borderRadius: 6,
            background: 'transparent',
            cursor: 'pointer',
            fontSize: 13,
          }}
        >
          ← Back
        </button>

        <div style={{ fontSize: 13, color: 'gray' }}>
          {metadata?.device_name ?? 'Connecting...'}
          {metadata && ` — ${metadata.width}×${metadata.height}`}
        </div>

        <div style={{ fontSize: 12, color: 'gray', fontFamily: 'monospace' }}>
          {status === 'active' && `${stats.fps} FPS`}
          {status === 'connecting' && 'Connecting...'}
          {status === 'error' && 'Error'}
        </div>
      </div>

      {/* Canvas area — fills remaining space */}
      <div style={{ flex: 1, padding: 16, overflow: 'hidden' }}>
        {status === 'error' ? (
          <div style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: 'center',
            justifyContent: 'center',
            height: '100%',
            gap: 16,
          }}>
            <div style={{ fontSize: 18, fontWeight: 500 }}>Mirror session ended</div>
            <div style={{ color: 'gray', fontSize: 14 }}>{error}</div>
            <button
              onClick={() => serial && start(serial)}
              style={{
                padding: '8px 20px',
                border: 'none',
                borderRadius: 8,
                background: 'var(--text-accent, #0066cc)',
                color: 'white',
                cursor: 'pointer',
                fontSize: 14,
              }}
            >
              Reconnect
            </button>
          </div>
        ) : (
          <MirrorCanvas
            canvasRef={canvasRef}
            deviceSize={deviceSize}
            active={status === 'active'}
          />
        )}
      </div>

      {/* Control bar */}
      <div style={{ flexShrink: 0, borderTop: '1px solid var(--border, #eee)' }}>
        <ControlBar
          active={status === 'active'}
          onScreenshot={handleScreenshot}
          showLatency={showLatency}
          latencyMs={null} // TODO: wire up latency measurement
        />
      </div>

      {/* Latency overlay (dev tool) */}
      {showLatency && status === 'active' && (
        <div
          style={{
            position: 'fixed',
            top: 48,
            right: 8,
            padding: '8px 12px',
            background: 'rgba(0,0,0,0.8)',
            color: '#0f0',
            fontFamily: 'monospace',
            fontSize: 11,
            borderRadius: 6,
            lineHeight: 1.6,
            zIndex: 100,
            pointerEvents: 'none',
          }}
        >
          <div>Frames received: {stats.framesReceived}</div>
          <div>Frames decoded: {stats.framesDecoded}</div>
          <div>Frames dropped: {stats.framesDropped}</div>
          <div>Decode queue: {stats.queueDepth}</div>
          <div>FPS: {stats.fps}</div>
          <div>Last pkt: {stats.lastPacketSize} B</div>
          <div>Elapsed: {stats.elapsedSeconds}s</div>
          <div style={{ marginTop: 4, color: '#888' }}>Ctrl+L to hide</div>
        </div>
      )}
    </div>
  );
}
