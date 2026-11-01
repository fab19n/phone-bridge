/**
 * ControlBar — navigation and utility buttons for the mirror view.
 *
 * Provides Android system navigation (Back, Home, Recent Apps),
 * volume control, screenshot, and recording controls.
 * Sits below the mirror canvas.
 */

import { useCallback } from 'react';
import * as ipc from '../../lib/ipc';

interface ControlBarProps {
  active: boolean;
  onScreenshot?: () => void;
  onToggleFullscreen?: () => void;
  showLatency?: boolean;
  latencyMs?: number | null;
}

export function ControlBar({
  active,
  onScreenshot,
  onToggleFullscreen,
  showLatency,
  latencyMs,
}: ControlBarProps) {
  const disabled = !active;

  const handleBack = useCallback(() => {
    if (disabled) return;
    ipc.sendButton('back');
  }, [disabled]);

  const handleHome = useCallback(() => {
    if (disabled) return;
    ipc.sendButton('home');
  }, [disabled]);

  const handleRecent = useCallback(() => {
    if (disabled) return;
    ipc.sendButton('recent');
  }, [disabled]);

  const handleVolumeUp = useCallback(() => {
    if (disabled) return;
    ipc.sendKey(ipc.KEYCODE.VOLUME_UP, 0);
    setTimeout(() => ipc.sendKey(ipc.KEYCODE.VOLUME_UP, 1), 50);
  }, [disabled]);

  const handleVolumeDown = useCallback(() => {
    if (disabled) return;
    ipc.sendKey(ipc.KEYCODE.VOLUME_DOWN, 0);
    setTimeout(() => ipc.sendKey(ipc.KEYCODE.VOLUME_DOWN, 1), 50);
  }, [disabled]);

  const handleScreenshot = useCallback(() => {
    if (disabled) return;
    onScreenshot?.();
  }, [disabled, onScreenshot]);

  const buttonStyle: React.CSSProperties = {
    padding: '8px 12px',
    border: '1px solid var(--border, #ddd)',
    borderRadius: 8,
    background: 'var(--surface-1, #f5f5f5)',
    cursor: disabled ? 'not-allowed' : 'pointer',
    opacity: disabled ? 0.4 : 1,
    fontSize: 13,
    fontWeight: 500,
    display: 'flex',
    alignItems: 'center',
    gap: 4,
    transition: 'opacity 0.15s',
  };

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 8,
        padding: '12px 0',
        flexWrap: 'wrap',
      }}
    >
      {/* Navigation group */}
      <div style={{ display: 'flex', gap: 4 }}>
        <button style={buttonStyle} onClick={handleBack} disabled={disabled} title="Back">
          ◀ Back
        </button>
        <button style={buttonStyle} onClick={handleHome} disabled={disabled} title="Home">
          ● Home
        </button>
        <button style={buttonStyle} onClick={handleRecent} disabled={disabled} title="Recent apps">
          ▢ Recent
        </button>
      </div>

      {/* Divider */}
      <div style={{ width: 1, height: 24, background: 'var(--border, #ddd)' }} />

      {/* Volume group */}
      <div style={{ display: 'flex', gap: 4 }}>
        <button style={buttonStyle} onClick={handleVolumeDown} disabled={disabled} title="Volume down">
          Vol -
        </button>
        <button style={buttonStyle} onClick={handleVolumeUp} disabled={disabled} title="Volume up">
          Vol +
        </button>
      </div>

      {/* Divider */}
      <div style={{ width: 1, height: 24, background: 'var(--border, #ddd)' }} />

      {/* Utility group */}
      <div style={{ display: 'flex', gap: 4 }}>
        <button style={buttonStyle} onClick={handleScreenshot} disabled={disabled} title="Screenshot">
          Screenshot
        </button>
        {onToggleFullscreen && (
          <button style={buttonStyle} onClick={onToggleFullscreen} title="Toggle fullscreen">
            Fullscreen
          </button>
        )}
      </div>

      {/* Latency indicator */}
      {showLatency && latencyMs != null && (
        <span style={{
          fontSize: 11,
          fontFamily: 'monospace',
          padding: '4px 8px',
          borderRadius: 4,
          background: latencyMs < 50 ? 'rgba(0,180,0,0.1)' :
                      latencyMs < 100 ? 'rgba(200,150,0,0.1)' :
                      'rgba(200,0,0,0.1)',
          color: latencyMs < 50 ? 'green' :
                 latencyMs < 100 ? 'orange' : 'red',
        }}>
          {latencyMs}ms
        </span>
      )}
    </div>
  );
}
