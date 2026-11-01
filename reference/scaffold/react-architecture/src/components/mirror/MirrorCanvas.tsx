/**
 * MirrorCanvas — the core mirroring display component.
 *
 * Renders the decoded phone screen on a <canvas> and captures
 * mouse/keyboard events to send as Android touch/key inputs.
 *
 * This component owns:
 *  - The <canvas> element where VideoFrames are drawn
 *  - Pointer event capture (mousedown/move/up → Android touch)
 *  - Keyboard event capture (when focused → Android key events)
 *  - Coordinate translation (CSS pixels → Android device pixels)
 *  - Aspect ratio preservation
 */

import { useRef, useCallback, useEffect } from 'react';
import { canvasToDevice, DeviceSize } from '../../lib/coordinates';
import * as ipc from '../../lib/ipc';

interface MirrorCanvasProps {
  /** Ref to the canvas — shared with useMirror for rendering */
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  /** Current device screen dimensions (from stream metadata) */
  deviceSize: DeviceSize | null;
  /** Whether the mirror session is active (enables input capture) */
  active: boolean;
  /** Optional className for styling */
  className?: string;
}

export function MirrorCanvas({ canvasRef, deviceSize, active, className }: MirrorCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  // -----------------------------------------------------------------------
  // Touch/pointer handling
  // -----------------------------------------------------------------------

  const handlePointerEvent = useCallback(
    (e: React.PointerEvent, action: ipc.TouchAction) => {
      if (!active || !deviceSize || !canvasRef.current) return;

      // Prevent browser gestures (scroll, zoom, text select)
      e.preventDefault();

      const coords = canvasToDevice(
        e.clientX,
        e.clientY,
        canvasRef.current,
        deviceSize,
      );

      if (!coords.inBounds) return;

      // Fire and forget — don't await (would add latency)
      ipc.sendTouch(coords.x, coords.y, action);
    },
    [active, deviceSize, canvasRef],
  );

  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      // Capture pointer for drag tracking outside canvas bounds
      canvasRef.current?.setPointerCapture(e.pointerId);
      handlePointerEvent(e, ipc.TouchAction.Down);
    },
    [handlePointerEvent, canvasRef],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      // Only send moves while a button is pressed (dragging)
      if (e.buttons === 0) return;
      handlePointerEvent(e, ipc.TouchAction.Move);
    },
    [handlePointerEvent],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent) => {
      handlePointerEvent(e, ipc.TouchAction.Up);
    },
    [handlePointerEvent],
  );

  // -----------------------------------------------------------------------
  // Keyboard handling
  // -----------------------------------------------------------------------

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!active) return;

      // Map common keys to Android keycodes
      // Full mapping would be extensive — this covers the essentials
      const keyMap: Record<string, number> = {
        Backspace: 67,
        Enter: 66,
        Tab: 61,
        Escape: 111,  // Maps to Android Back
        ArrowUp: 19,
        ArrowDown: 20,
        ArrowLeft: 21,
        ArrowRight: 22,
        Delete: 112,
        Home: 122,
        End: 123,
        PageUp: 92,
        PageDown: 93,
        ' ': 62,
      };

      if (e.key === 'Escape') {
        // Escape → Android Back button
        e.preventDefault();
        ipc.sendButton('back');
        return;
      }

      const keycode = keyMap[e.key];
      if (keycode) {
        e.preventDefault();
        ipc.sendKey(keycode, 0); // 0 = ACTION_DOWN
        return;
      }

      // For printable characters, use text injection
      // This handles Unicode, IME, etc. better than key-by-key injection
      if (e.key.length === 1 && !e.ctrlKey && !e.metaKey) {
        e.preventDefault();
        ipc.sendText(e.key);
      }
    },
    [active],
  );

  const onKeyUp = useCallback(
    (e: React.KeyboardEvent) => {
      if (!active) return;

      const keyMap: Record<string, number> = {
        Backspace: 67, Enter: 66, Tab: 61,
        ArrowUp: 19, ArrowDown: 20, ArrowLeft: 21, ArrowRight: 22,
        Delete: 112, ' ': 62,
      };

      const keycode = keyMap[e.key];
      if (keycode) {
        ipc.sendKey(keycode, 1); // 1 = ACTION_UP
      }
    },
    [active],
  );

  // -----------------------------------------------------------------------
  // Auto-focus canvas when mirror becomes active
  // -----------------------------------------------------------------------

  useEffect(() => {
    if (active && canvasRef.current) {
      canvasRef.current.focus();
    }
  }, [active, canvasRef]);

  // -----------------------------------------------------------------------
  // Scroll handling (maps to Android scroll events)
  // -----------------------------------------------------------------------

  const onWheel = useCallback(
    (e: React.WheelEvent) => {
      if (!active || !deviceSize || !canvasRef.current) return;
      e.preventDefault();

      const coords = canvasToDevice(
        e.clientX,
        e.clientY,
        canvasRef.current,
        deviceSize,
      );

      if (!coords.inBounds) return;

      // Normalize scroll delta to -1/0/+1
      const scrollX = Math.sign(e.deltaX);
      const scrollY = Math.sign(e.deltaY);

      // sendScroll would be a separate IPC command
      // For Phase 1, scroll via the scrcpy scroll control message
      invoke('send_scroll', { x: coords.x, y: coords.y, scrollX, scrollY }).catch(() => {});
    },
    [active, deviceSize, canvasRef],
  );

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------

  // Calculate display dimensions preserving aspect ratio
  const aspectRatio = deviceSize
    ? deviceSize.width / deviceSize.height
    : 9 / 16; // default phone aspect

  return (
    <div
      ref={containerRef}
      className={className}
      style={{
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'center',
        width: '100%',
        height: '100%',
        overflow: 'hidden',
      }}
    >
      <canvas
        ref={canvasRef}
        tabIndex={0}  // Makes canvas focusable for keyboard events
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onKeyDown={onKeyDown}
        onKeyUp={onKeyUp}
        onWheel={onWheel}
        style={{
          maxWidth: '100%',
          maxHeight: '100%',
          objectFit: 'contain',
          cursor: active ? 'default' : 'not-allowed',
          touchAction: 'none',  // Prevent browser gesture interference
          outline: 'none',      // Remove focus ring (canvas handles focus internally)
          aspectRatio: `${aspectRatio}`,
          backgroundColor: '#0a0a0a',
          borderRadius: 12,
        }}
      />
    </div>
  );
}

// Missing import — would normally be at the top
import { invoke } from '@tauri-apps/api/core';
