/**
 * Dashboard — the landing page.
 *
 * Shows:
 *  - Connection status (disconnected / detected / ready)
 *  - Paired device list with Mirror and Files buttons
 *  - Setup guidance when no device is connected
 *
 * This page does NOT start any resource-intensive operations.
 * Mirroring only begins when the user navigates to the Mirror page.
 */

import { useEffect, useState, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import * as ipc from '../lib/ipc';

export function DashboardPage() {
  const navigate = useNavigate();
  const [connectionState, setConnectionState] = useState<ipc.ConnectionState>({ state: 'Disconnected' });
  const [pairedDevices, setPairedDevices] = useState<ipc.PairedDevice[]>([]);

  // Listen for connection state changes
  useEffect(() => {
    const unlistenPromise = ipc.onConnectionStateChanged(setConnectionState);
    return () => { unlistenPromise.then(fn => fn()); };
  }, []);

  // Load paired devices on mount
  useEffect(() => {
    ipc.getPairedDevices().then(setPairedDevices).catch(console.error);
  }, [connectionState]); // Refresh when connection state changes (new pairing)

  // Detect devices on mount
  useEffect(() => {
    ipc.detectDevices().catch(console.error);
  }, []);

  const handleMirror = useCallback((serial: string) => {
    navigate(`/mirror/${serial}`);
  }, [navigate]);

  const handlePair = useCallback(async (serial: string) => {
    try {
      const device = await ipc.pairDevice(serial);
      setPairedDevices(prev => [...prev, device]);
    } catch (e) {
      console.error('Pairing failed:', e);
    }
  }, []);

  const handleUnpair = useCallback(async (deviceId: string) => {
    try {
      await ipc.unpairDevice(deviceId);
      setPairedDevices(prev => prev.filter(d => d.id !== deviceId));
    } catch (e) {
      console.error('Unpair failed:', e);
    }
  }, []);

  return (
    <div style={{ padding: 24, maxWidth: 480, margin: '0 auto' }}>
      {/* Header */}
      <div style={{ marginBottom: 32 }}>
        <h1 style={{ fontSize: 24, margin: '0 0 4px', fontWeight: 600 }}>PhoneBridge</h1>
        <p style={{ margin: 0, color: 'gray', fontSize: 14 }}>
          Mirror and control your Android phone
        </p>
      </div>

      {/* Connection status */}
      <ConnectionStatus state={connectionState} onPair={handlePair} />

      {/* Paired devices */}
      {pairedDevices.length > 0 && (
        <div style={{ marginTop: 24 }}>
          <h2 style={{ fontSize: 16, margin: '0 0 12px', fontWeight: 500 }}>
            Paired devices
          </h2>
          {pairedDevices.map(device => (
            <DeviceCard
              key={device.id}
              device={device}
              connectionState={connectionState}
              onMirror={() => handleMirror(device.serial)}
              onUnpair={() => handleUnpair(device.id)}
            />
          ))}
        </div>
      )}

      {/* Empty state */}
      {pairedDevices.length === 0 && connectionState.state === 'Disconnected' && (
        <div style={{
          marginTop: 48,
          textAlign: 'center',
          padding: 32,
          border: '1px dashed var(--border, #ddd)',
          borderRadius: 12,
        }}>
          <div style={{ fontSize: 32, marginBottom: 12 }}>📱</div>
          <p style={{ margin: '0 0 8px', fontWeight: 500 }}>No devices paired</p>
          <p style={{ margin: 0, color: 'gray', fontSize: 13, lineHeight: 1.5 }}>
            Connect your Android phone via USB cable.
            Make sure USB debugging is enabled in Developer Options.
          </p>
        </div>
      )}

      {/* Settings link */}
      <div style={{ marginTop: 32, textAlign: 'center' }}>
        <button
          onClick={() => navigate('/settings')}
          style={{
            background: 'none',
            border: 'none',
            color: 'gray',
            cursor: 'pointer',
            fontSize: 13,
            textDecoration: 'underline',
          }}
        >
          Settings
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function ConnectionStatus({
  state,
  onPair,
}: {
  state: ipc.ConnectionState;
  onPair: (serial: string) => void;
}) {
  if (state.state === 'Disconnected') return null;

  const colors: Record<string, { bg: string; text: string; border: string }> = {
    DetectedUnpaired: { bg: '#fff8e1', text: '#f57f17', border: '#ffcc02' },
    Ready: { bg: '#e8f5e9', text: '#2e7d32', border: '#66bb6a' },
    Connecting: { bg: '#e3f2fd', text: '#1565c0', border: '#42a5f5' },
    Mirroring: { bg: '#e8f5e9', text: '#2e7d32', border: '#66bb6a' },
    Error: { bg: '#ffebee', text: '#c62828', border: '#ef5350' },
    Stopped: { bg: '#fff3e0', text: '#e65100', border: '#ff9800' },
  };

  const style = colors[state.state] ?? colors.Stopped;

  return (
    <div style={{
      padding: 16,
      borderRadius: 10,
      border: `1px solid ${style.border}`,
      background: style.bg,
    }}>
      <div style={{ fontWeight: 500, color: style.text, marginBottom: 4 }}>
        {state.state === 'DetectedUnpaired' && '📱 New device detected'}
        {state.state === 'Ready' && '✓ Device ready'}
        {state.state === 'Connecting' && '⟳ Connecting...'}
        {state.state === 'Mirroring' && '● Mirroring active'}
        {state.state === 'Error' && '✕ Error'}
        {state.state === 'Stopped' && '⏸ Session ended'}
      </div>

      {'serial' in state && (
        <div style={{ fontSize: 12, color: style.text, opacity: 0.7 }}>
          {state.serial}
        </div>
      )}

      {state.state === 'DetectedUnpaired' && (
        <button
          onClick={() => onPair(state.serial)}
          style={{
            marginTop: 8,
            padding: '6px 16px',
            border: `1px solid ${style.border}`,
            borderRadius: 6,
            background: 'white',
            cursor: 'pointer',
            fontSize: 13,
            fontWeight: 500,
            color: style.text,
          }}
        >
          Pair this device
        </button>
      )}

      {state.state === 'Error' && 'message' in state && (
        <div style={{ fontSize: 12, marginTop: 4 }}>{state.message}</div>
      )}
    </div>
  );
}

function DeviceCard({
  device,
  connectionState,
  onMirror,
  onUnpair,
}: {
  device: ipc.PairedDevice;
  connectionState: ipc.ConnectionState;
  onMirror: () => void;
  onUnpair: () => void;
}) {
  // Is this device currently connected?
  const isConnected =
    'serial' in connectionState &&
    connectionState.serial === device.serial &&
    ['Ready', 'Connecting', 'Mirroring'].includes(connectionState.state);

  return (
    <div style={{
      padding: 16,
      border: '1px solid var(--border, #e0e0e0)',
      borderRadius: 10,
      marginBottom: 8,
      display: 'flex',
      alignItems: 'center',
      gap: 12,
    }}>
      <div style={{ flex: 1 }}>
        <div style={{ fontWeight: 500 }}>{device.name}</div>
        <div style={{ fontSize: 12, color: 'gray' }}>
          {isConnected ? 'Connected via USB' : `Last seen: ${formatAge(device.last_seen)}`}
        </div>
      </div>

      {isConnected && (
        <button
          onClick={onMirror}
          style={{
            padding: '8px 20px',
            border: 'none',
            borderRadius: 8,
            background: '#0066cc',
            color: 'white',
            cursor: 'pointer',
            fontWeight: 500,
            fontSize: 14,
          }}
        >
          Mirror
        </button>
      )}

      <button
        onClick={onUnpair}
        style={{
          padding: '8px 12px',
          border: '1px solid #ddd',
          borderRadius: 8,
          background: 'transparent',
          cursor: 'pointer',
          fontSize: 12,
          color: 'gray',
        }}
      >
        Remove
      </button>
    </div>
  );
}

function formatAge(timestamp: number): string {
  const diff = Date.now() - timestamp;
  if (diff < 60_000) return 'just now';
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
}
