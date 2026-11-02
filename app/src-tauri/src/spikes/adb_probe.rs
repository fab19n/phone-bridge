//! Test 0G: use the user's running adb server over TCP 5037 without restarting it (REVIEW B10). The
//! probe never runs the adb CLI; it compares the adb.exe process ids before and after.

use std::time::Duration;

use phonebridge_adb::{AdbServer, DeviceState};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbProbeReport {
    server: String,
    version: Option<u32>,
    devices: Vec<ProbeDevice>,
    /// Device lists received from `host:track-devices` while the probe listened.
    track_updates: Vec<Vec<String>>,
    adb_pids_before: Vec<u32>,
    adb_pids_after: Vec<u32>,
    /// The same adb server process was running before and after the probe.
    server_untouched: bool,
    errors: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeDevice {
    serial: String,
    state: String,
    model: Option<String>,
    features: Vec<String>,
}

#[tauri::command]
pub async fn spike_adb_probe(track_ms: u64) -> Result<AdbProbeReport, String> {
    let adb_pids_before = adb_pids();
    let server = AdbServer::from_env();
    let mut errors = Vec::new();

    let version = match server.version().await {
        Ok(v) => Some(v),
        Err(e) => {
            errors.push(format!("version: {e}"));
            None
        }
    };

    let mut devices = Vec::new();
    match server.devices().await {
        Ok(list) => {
            for device in list {
                let features = if device.state == DeviceState::Device {
                    server.features(&device.serial).await.unwrap_or_else(|e| {
                        errors.push(format!("features {}: {e}", device.serial));
                        Vec::new()
                    })
                } else {
                    Vec::new()
                };
                devices.push(ProbeDevice {
                    serial: device.serial,
                    state: format!("{:?}", device.state),
                    model: device.model,
                    features,
                });
            }
        }
        Err(e) => errors.push(format!("devices: {e}")),
    }

    let mut track_updates = Vec::new();
    match server.track_devices().await {
        Ok(mut tracker) => {
            let deadline =
                tokio::time::Instant::now() + Duration::from_millis(track_ms.min(30_000));
            while let Ok(next) = tokio::time::timeout_at(deadline, tracker.next()).await {
                match next {
                    Ok(Some(list)) => track_updates.push(
                        list.into_iter()
                            .map(|(serial, state)| format!("{serial} {state:?}"))
                            .collect(),
                    ),
                    Ok(None) => break,
                    Err(e) => {
                        errors.push(format!("track-devices: {e}"));
                        break;
                    }
                }
            }
        }
        Err(e) => errors.push(format!("track-devices: {e}")),
    }

    let adb_pids_after = adb_pids();
    Ok(AdbProbeReport {
        server: server.addr().to_string(),
        version,
        devices,
        track_updates,
        server_untouched: !adb_pids_before.is_empty() && adb_pids_before == adb_pids_after,
        adb_pids_before,
        adb_pids_after,
        errors,
    })
}

/// Process ids of running adb.exe processes, from `tasklist` (run without a console window).
#[cfg(windows)]
pub(crate) fn adb_pids() -> Vec<u32> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let Ok(out) = std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq adb.exe", "/FO", "CSV", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return Vec::new();
    };
    // CSV rows: "adb.exe","42004","Console","6","17,140 K"
    let mut pids: Vec<u32> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.split(',').nth(1)?.trim_matches('"').parse().ok())
        .collect();
    pids.sort_unstable();
    pids
}

#[cfg(not(windows))]
pub(crate) fn adb_pids() -> Vec<u32> {
    Vec::new()
}
