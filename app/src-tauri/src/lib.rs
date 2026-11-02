#[cfg(feature = "spikes")]
mod spikes;
// Used only by the Stage 1 spikes until Stage 2's mirror session lands.
#[cfg_attr(not(feature = "spikes"), allow(dead_code))]
mod video_channel;

/// Reports build/version info so the frontend can show what it is talking to.
#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "scrcpyServerVersion": phonebridge_scrcpy::SERVER_VERSION,
        "spikes": cfg!(feature = "spikes"),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "phonebridge=info".into()),
        )
        .init();

    let builder = tauri::Builder::default();

    #[cfg(feature = "spikes")]
    let builder = builder
        .manage(spikes::ordering::OrderingState::default())
        .invoke_handler(tauri::generate_handler![
            app_info,
            spikes::adb_probe::spike_adb_probe,
            spikes::mirror::spike_mirror,
            spikes::mirror::spike_replay,
            spikes::ordering::spike_ordering_start,
            spikes::ordering::spike_ordering_send_async,
            spikes::ordering::spike_ordering_send_sync,
            spikes::ordering::spike_ordering_send_batch,
            spikes::ordering::spike_ordering_finish,
            spikes::throughput::spike_throughput,
        ]);

    #[cfg(not(feature = "spikes"))]
    let builder = builder.invoke_handler(tauri::generate_handler![app_info]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
