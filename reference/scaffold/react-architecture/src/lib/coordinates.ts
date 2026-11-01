/**
 * Coordinate translation between the HTML canvas and the Android device screen.
 *
 * The canvas displays the phone screen scaled to fit the browser window.
 * When the user clicks at (clientX, clientY) on the canvas, we need to
 * calculate the corresponding (x, y) on the Android device's actual screen.
 *
 * This handles:
 *  - Canvas CSS size vs device resolution scaling
 *  - Aspect ratio letterboxing (if canvas aspect differs from device)
 *  - Device orientation changes
 *  - DPI scaling
 */

export interface DeviceSize {
  width: number;   // device screen width in pixels
  height: number;  // device screen height in pixels
}

export interface CanvasCoords {
  /** Position on the Android device screen (0,0 = top-left) */
  x: number;
  y: number;
  /** Whether the click is within the device screen bounds */
  inBounds: boolean;
}

/**
 * Translate a pointer event on the canvas to device coordinates.
 *
 * @param event - The React pointer event (or native PointerEvent)
 * @param canvas - The canvas element displaying the mirror
 * @param deviceSize - The Android device's actual screen resolution
 * @returns Device coordinates, or inBounds=false if outside the visible area
 */
export function canvasToDevice(
  clientX: number,
  clientY: number,
  canvas: HTMLCanvasElement,
  deviceSize: DeviceSize,
): CanvasCoords {
  const rect = canvas.getBoundingClientRect();

  // Position relative to canvas element (in CSS pixels)
  const cssX = clientX - rect.left;
  const cssY = clientY - rect.top;

  // The canvas element's CSS size
  const cssWidth = rect.width;
  const cssHeight = rect.height;

  // The canvas buffer's pixel size (should match device resolution)
  const bufferWidth = canvas.width;
  const bufferHeight = canvas.height;

  // Scale from CSS pixels to canvas buffer pixels
  // This accounts for CSS scaling (e.g., max-width: 100%, object-fit: contain)
  const scaleX = bufferWidth / cssWidth;
  const scaleY = bufferHeight / cssHeight;

  // Canvas buffer coordinates
  const bufX = cssX * scaleX;
  const bufY = cssY * scaleY;

  // If the canvas buffer matches the device size (the normal case),
  // the buffer coords ARE the device coords.
  // If they differ (e.g., scrcpy's max_size capped the resolution),
  // scale from buffer to device.
  const deviceX = (bufX / bufferWidth) * deviceSize.width;
  const deviceY = (bufY / bufferHeight) * deviceSize.height;

  // Bounds check
  const inBounds =
    deviceX >= 0 &&
    deviceX < deviceSize.width &&
    deviceY >= 0 &&
    deviceY < deviceSize.height;

  return {
    x: Math.round(Math.max(0, Math.min(deviceX, deviceSize.width - 1))),
    y: Math.round(Math.max(0, Math.min(deviceY, deviceSize.height - 1))),
    inBounds,
  };
}
