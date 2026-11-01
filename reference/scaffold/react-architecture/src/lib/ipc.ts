/**
 * Typed IPC layer — all communication with the Rust backend goes through here.
 *
 * This module provides:
 *  - Type-safe wrappers around Tauri's invoke() and Channel
 *  - All command signatures in one place (single source of truth)
 *  - Types matching the Rust-side serde serialization
 *
 * Never call invoke() directly from components — use these functions.
 */

import { invoke, Channel } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

// ---------------------------------------------------------------------------
// Types matching Rust structs
// ---------------------------------------------------------------------------

export interface Device {
  serial: string;
  model: string;
  status: 'device' | 'unauthorized' | 'offline';
}

export interface PairedDevice {
  id: string;
  name: string;
  serial: string;
  public_key: string;
  paired_at: number;
  last_seen: number;
  connection_count: number;
}

export interface StreamMetadata {
  device_name: string;
  width: number;
  height: number;
  codec_id: number;
}

export interface PhoneBridgeSettings {
  maxFps: 30 | 60;
  maxSize: 1080 | 1440 | 1920;
  videoBitRate: number;
  videoCodec: 'h264';
  autoConnect: boolean;
  startMinimized: boolean;
  minimizeToTray: boolean;
  screenshotDir: string;
  recordingDir: string;
  showLatencyOverlay: boolean;
}

// Video events from Rust via Channel
export type VideoEvent =
  | { type: 'metadata'; device_name: string; width: number; height: number; codec_id: number }
  | { type: 'config'; data: ArrayBuffer }
  | { type: 'frame'; pts: number; keyframe: boolean; data: ArrayBuffer; received_us: number }
  | { type: 'stopped'; reason: string };

// Connection state from Rust via events
export type ConnectionState =
  | { state: 'Disconnected' }
  | { state: 'DetectedUnpaired'; serial: string; model: string }
  | { state: 'Ready'; serial: string; device: PairedDevice }
  | { state: 'Connecting'; serial: string }
  | { state: 'Mirroring'; serial: string; latency_ms: number | null }
  | { state: 'Stopped'; serial: string; reason: string }
  | { state: 'Error'; message: string; recoverable: boolean };

// ---------------------------------------------------------------------------
// Device commands
// ---------------------------------------------------------------------------

export async function detectDevices(): Promise<Device[]> {
  return invoke<Device[]>('detect_devices');
}

export async function getPairedDevices(): Promise<PairedDevice[]> {
  return invoke<PairedDevice[]>('get_paired_devices');
}

export async function pairDevice(serial: string): Promise<PairedDevice> {
  return invoke<PairedDevice>('pair_device', { serial });
}

export async function unpairDevice(deviceId: string): Promise<void> {
  return invoke('unpair_device', { deviceId });
}

// ---------------------------------------------------------------------------
// Mirror commands
// ---------------------------------------------------------------------------

export async function startMirror(
  serial: string,
  onEvent: (event: VideoEvent) => void,
): Promise<void> {
  const channel = new Channel<VideoEvent>();
  channel.onmessage = onEvent;
  return invoke('start_mirror', { serial, onEvent: channel });
}

export async function stopMirror(): Promise<void> {
  return invoke('stop_mirror');
}

// ---------------------------------------------------------------------------
// Control commands
// ---------------------------------------------------------------------------

/** Touch actions matching scrcpy's AMOTION_EVENT_ACTION constants */
export enum TouchAction {
  Down = 0,
  Up = 1,
  Move = 2,
}

export async function sendTouch(x: number, y: number, action: TouchAction): Promise<void> {
  return invoke('send_touch', { x, y, action });
}

export async function sendKey(keycode: number, action: 0 | 1): Promise<void> {
  return invoke('send_key', { keycode, action });
}

export async function sendButton(button: 'back' | 'home' | 'recent'): Promise<void> {
  return invoke('send_button', { button });
}

export async function sendText(text: string): Promise<void> {
  return invoke('send_text', { text });
}

// Android keycodes for common buttons
export const KEYCODE = {
  BACK: 4,
  HOME: 3,
  APP_SWITCH: 187,  // Recent apps
  VOLUME_UP: 24,
  VOLUME_DOWN: 25,
  POWER: 26,
} as const;

// ---------------------------------------------------------------------------
// Screenshot
// ---------------------------------------------------------------------------

export async function takeScreenshot(): Promise<string> {
  // Returns the saved file path
  return invoke<string>('take_screenshot');
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

export async function getSettings(): Promise<PhoneBridgeSettings> {
  return invoke<PhoneBridgeSettings>('get_settings');
}

export async function setSetting(key: string, value: unknown): Promise<void> {
  return invoke('set_setting', { key, value });
}

// ---------------------------------------------------------------------------
// Event listeners
// ---------------------------------------------------------------------------

export function onConnectionStateChanged(
  handler: (state: ConnectionState) => void,
): Promise<UnlistenFn> {
  return listen<ConnectionState>('connection:state-changed', (event) => {
    handler(event.payload);
  });
}

export function onDeviceConnected(
  handler: (serial: string) => void,
): Promise<UnlistenFn> {
  return listen<string>('device:connected', (event) => {
    handler(event.payload);
  });
}

export function onDeviceDisconnected(
  handler: (serial: string) => void,
): Promise<UnlistenFn> {
  return listen<string>('device:disconnected', (event) => {
    handler(event.payload);
  });
}
