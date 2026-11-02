//! Against the adb server running on this machine. Read-only requests only: this never starts,
//! stops or restarts the server, and never runs the adb CLI.

use phonebridge_adb::AdbServer;

#[tokio::test]
#[ignore = "needs a running adb server (127.0.0.1:5037, or where ADB_SERVER_SOCKET etc. say)"]
async fn real_server_reports_version_and_devices() {
    let server = AdbServer::from_env();
    let version = server.version().await.unwrap();
    assert!(
        (32..100).contains(&version),
        "implausible adb server version {version}"
    );
    let devices = server.devices().await.unwrap();
    let states: Vec<_> = devices.iter().map(|device| &device.state).collect();
    println!(
        "adb server {} is version {version}; device states: {states:?}",
        server.addr()
    );
}
