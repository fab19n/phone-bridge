//! The client against an in-process fake adb server that speaks just enough of the protocol. The
//! fake side writes its replies byte by byte as AOSP does (`adb.cpp`, `sockets.cpp`,
//! `daemon/file_sync_service.cpp`), independently of the crate's own encoders. The one reply AOSP
//! never sends is labelled as such.

use std::path::Path;
use std::time::Duration;

use phonebridge_adb::{
    AdbServer, Device, DeviceState, Error, ForwardRule, ShellEvent, Target, ensure_server,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

/// Serves one connection with `handler` and hands back what it returns.
async fn serve<F, Fut, T>(handler: F) -> (AdbServer, JoinHandle<T>)
where
    F: FnOnce(TcpStream) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    serve_connections(|listener| async move { handler(accept(&listener).await).await }).await
}

/// Hands the listener to `handler`, for a client that connects more than once.
async fn serve_connections<F, Fut, T>(handler: F) -> (AdbServer, JoinHandle<T>)
where
    F: FnOnce(TcpListener) -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = AdbServer::new(listener.local_addr().unwrap());
    (server, tokio::spawn(handler(listener)))
}

async fn accept(listener: &TcpListener) -> TcpStream {
    listener.accept().await.unwrap().0
}

/// True if no further connection arrives. Call it once the client has returned: any connection it
/// made is then already waiting in the backlog.
async fn no_more_connections(listener: &TcpListener) -> bool {
    tokio::time::timeout(Duration::from_millis(100), listener.accept())
        .await
        .is_err()
}

/// A server that nothing listens for: a call that reaches the network fails with `Error::Connect`.
async fn unreachable() -> AdbServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    AdbServer::new(addr)
}

/// One request: 4 hex digits of length, then the service.
async fn read_request(stream: &mut TcpStream) -> String {
    let mut len = [0; 4];
    stream.read_exact(&mut len).await.unwrap();
    let len = usize::from_str_radix(std::str::from_utf8(&len).unwrap(), 16).unwrap();
    let mut service = vec![0; len];
    stream.read_exact(&mut service).await.unwrap();
    String::from_utf8(service).unwrap()
}

async fn send(stream: &mut TcpStream, bytes: &[u8]) {
    stream.write_all(bytes).await.unwrap();
}

async fn okay(stream: &mut TcpStream) {
    send(stream, b"OKAY").await;
}

async fn fail(stream: &mut TcpStream, message: &str) {
    send(
        stream,
        format!("FAIL{:04x}{message}", message.len()).as_bytes(),
    )
    .await;
}

async fn block(stream: &mut TcpStream, payload: &str) {
    send(stream, format!("{:04x}{payload}", payload.len()).as_bytes()).await;
}

/// Accepts `host:transport:<serial>`, then `service`.
async fn accept_transport(stream: &mut TcpStream, serial: &str, service: &str) {
    assert_eq!(
        read_request(stream).await,
        format!("host:transport:{serial}")
    );
    okay(stream).await;
    assert_eq!(read_request(stream).await, service);
    okay(stream).await;
}

/// Accepts `host:transport:<serial>` and returns the request that follows it, unanswered.
async fn accept_switch(stream: &mut TcpStream, serial: &str) -> String {
    assert_eq!(
        read_request(stream).await,
        format!("host:transport:{serial}")
    );
    okay(stream).await;
    read_request(stream).await
}

/// Everything the peer sends until it closes.
async fn read_to_end(stream: &mut TcpStream) -> Vec<u8> {
    let mut rest = Vec::new();
    stream.read_to_end(&mut rest).await.unwrap();
    rest
}

const SERIAL: &str = "R58M123ABC";
const SOCKET: &str = "localabstract:scrcpy_1a2b3c4d";

#[tokio::test]
async fn version_is_host_version_answered_in_hex() {
    let (server, fake) = serve(|mut s| async move {
        let mut request = [0; 16];
        s.read_exact(&mut request).await.unwrap();
        okay(&mut s).await;
        block(&mut s, "0029").await;
        request
    })
    .await;
    assert_eq!(server.version().await.unwrap(), 41);
    assert_eq!(&fake.await.unwrap(), b"000chost:version");
}

#[tokio::test]
async fn devices_parses_devices_l() {
    let (server, fake) = serve(|mut s| async move {
        assert_eq!(read_request(&mut s).await, "host:devices-l");
        okay(&mut s).await;
        block(
            &mut s,
            "R58M123ABC             device usb:1-4 product:beyond1qltesq model:SM_G973U device:beyond1q transport_id:3\n\
             0123456789ABCDEF       unauthorized usb:1-2 transport_id:4\n\
             ZY22ABCDEF             no permissions (missing udev rules? user is in the plugdev group); \
             see [http://developer.android.com/tools/device.html] usb:1-1 transport_id:5\n",
        )
        .await;
    })
    .await;
    let devices = server.devices().await.unwrap();
    fake.await.unwrap();
    assert_eq!(
        devices,
        [
            Device {
                serial: SERIAL.into(),
                state: DeviceState::Device,
                product: Some("beyond1qltesq".into()),
                model: Some("SM_G973U".into()),
                device: Some("beyond1q".into()),
                transport_id: Some(3),
            },
            Device {
                serial: "0123456789ABCDEF".into(),
                state: DeviceState::Unauthorized,
                product: None,
                model: None,
                device: None,
                transport_id: Some(4),
            },
            Device {
                serial: "ZY22ABCDEF".into(),
                state: DeviceState::NoPermissions,
                product: None,
                model: None,
                device: None,
                transport_id: Some(5),
            },
        ]
    );
}

#[tokio::test]
async fn track_devices_yields_each_list_then_none() {
    let (server, fake) = serve(|mut s| async move {
        assert_eq!(read_request(&mut s).await, "host:track-devices");
        okay(&mut s).await;
        block(&mut s, "R58M123ABC\tunauthorized\n").await;
        block(&mut s, "R58M123ABC\tdevice\nemulator-5554\toffline\n").await;
    })
    .await;
    let mut tracker = server.track_devices().await.unwrap();
    assert_eq!(
        tracker.next().await.unwrap(),
        Some(vec![(SERIAL.to_owned(), DeviceState::Unauthorized)])
    );
    assert_eq!(
        tracker.next().await.unwrap(),
        Some(vec![
            (SERIAL.to_owned(), DeviceState::Device),
            ("emulator-5554".to_owned(), DeviceState::Offline),
        ])
    );
    assert_eq!(tracker.next().await.unwrap(), None);
    fake.await.unwrap();
}

#[tokio::test]
async fn track_devices_cut_mid_list_is_an_error() {
    let (server, fake) = serve(|mut s| async move {
        assert_eq!(read_request(&mut s).await, "host:track-devices");
        okay(&mut s).await;
        send(&mut s, b"0017R58M123ABC\tdev").await;
    })
    .await;
    let mut tracker = server.track_devices().await.unwrap();
    assert!(matches!(tracker.next().await, Err(Error::Protocol(_))));
    fake.await.unwrap();
}

#[tokio::test]
async fn features_are_host_serial_features() {
    let (server, fake) = serve(|mut s| async move {
        assert_eq!(
            read_request(&mut s).await,
            "host-serial:R58M123ABC:features"
        );
        okay(&mut s).await;
        block(
            &mut s,
            "shell_v2,cmd,stat_v2,ls_v2,fixed_push_mkdir,apex,abb,fixed_push_symlink_timestamp,\
             abb_exec,remount_shell,track_app,sendrecv_v2,sendrecv_v2_brotli,sendrecv_v2_lz4,\
             sendrecv_v2_zstd,sendrecv_v2_dry_run_send,openscreen_mdns,delayed_ack",
        )
        .await;
    })
    .await;
    let features = server.features(SERIAL).await.unwrap();
    fake.await.unwrap();
    assert_eq!(features.len(), 18);
    assert_eq!(&features[..3], ["shell_v2", "cmd", "stat_v2"]);
    assert!(features.iter().any(|f| f == "delayed_ack"));
}

#[tokio::test]
async fn open_switches_transport_then_streams_raw_bytes_both_ways() {
    let (server, fake) = serve(|mut s| async move {
        assert_eq!(read_request(&mut s).await, "host:transport:R58M123ABC");
        okay(&mut s).await;
        assert_eq!(read_request(&mut s).await, SOCKET);
        // The service's first byte (scrcpy's dummy byte) right behind the OKAY, in one write.
        send(&mut s, b"OKAY\0").await;
        let mut ping = [0; 4];
        s.read_exact(&mut ping).await.unwrap();
        send(&mut s, b"pong").await;
        ping
    })
    .await;
    let mut stream = server.open(SERIAL, SOCKET).await.unwrap();
    assert!(stream.nodelay().unwrap());
    let mut dummy = [0xff];
    stream.read_exact(&mut dummy).await.unwrap();
    assert_eq!(dummy, [0]);
    stream.write_all(b"ping").await.unwrap();
    let mut pong = [0; 4];
    stream.read_exact(&mut pong).await.unwrap();
    assert_eq!(&pong, b"pong");
    assert_eq!(&fake.await.unwrap(), b"ping");
}

#[tokio::test]
async fn transport_fail_is_error_fail() {
    let (server, fake) = serve(|mut s| async move {
        assert_eq!(read_request(&mut s).await, "host:transport:x");
        fail(&mut s, "device 'x' not found").await;
        read_to_end(&mut s).await
    })
    .await;
    let error = server.open("x", SOCKET).await.unwrap_err();
    assert!(
        matches!(&error, Error::Fail(m) if m == "device 'x' not found"),
        "{error:?}"
    );
    // The service request is never sent after a failed switch.
    assert_eq!(fake.await.unwrap(), b"");
}

#[tokio::test]
async fn service_fail_after_transport_is_error_fail() {
    // What the server sends while nothing listens on the device socket yet
    // (`local_socket_close_notify` in `sockets.cpp`).
    let (server, fake) = serve(|mut s| async move {
        assert_eq!(read_request(&mut s).await, "host:transport:R58M123ABC");
        okay(&mut s).await;
        assert_eq!(read_request(&mut s).await, SOCKET);
        fail(&mut s, "closed").await;
    })
    .await;
    let error = server.open(SERIAL, SOCKET).await.unwrap_err();
    assert!(
        matches!(&error, Error::Fail(m) if m == "closed"),
        "{error:?}"
    );
    fake.await.unwrap();
}

#[tokio::test]
async fn a_device_without_a_serial_is_reached_by_transport_id() {
    // No serial request matches `(no serial number)` (`atransport::MatchesTarget`).
    let (server, fake) = serve_connections(|listener| async move {
        let mut s = accept(&listener).await;
        assert_eq!(read_request(&mut s).await, "host:devices-l");
        okay(&mut s).await;
        block(
            &mut s,
            "(no serial number)     device usb:1-5 product:p model:m device:d transport_id:11\n",
        )
        .await;
        let mut s = accept(&listener).await;
        assert_eq!(read_request(&mut s).await, "host-transport-id:11:features");
        okay(&mut s).await;
        block(&mut s, "shell_v2,cmd").await;
        let mut s = accept(&listener).await;
        assert_eq!(read_request(&mut s).await, "host:transport-id:11");
        okay(&mut s).await;
        assert_eq!(read_request(&mut s).await, SOCKET);
        okay(&mut s).await;
    })
    .await;
    let devices = server.devices().await.unwrap();
    let target = devices[0].target();
    assert_eq!(target, Target::TransportId(11));
    assert_eq!(
        server.features_on(&target).await.unwrap(),
        ["shell_v2", "cmd"]
    );
    server.open_on(&target, SOCKET).await.unwrap();
    fake.await.unwrap();
}

#[tokio::test]
async fn a_stale_transport_id_is_error_fail() {
    let (server, fake) = serve(|mut s| async move {
        assert_eq!(read_request(&mut s).await, "host:transport-id:12");
        fail(&mut s, "no device with transport id '12'").await;
    })
    .await;
    let error = server
        .open_on(&Target::TransportId(12), SOCKET)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, Error::Fail(m) if m == "no device with transport id '12'"),
        "{error:?}"
    );
    fake.await.unwrap();
}

/// A shell v2 frame: 1-byte id, u32 little-endian length, payload.
fn shell_frame(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![id];
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    frame
}

#[tokio::test]
async fn shell_v2_separates_stdout_stderr_and_exit() {
    const COMMAND: &str = "CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / \
                           com.genymobile.scrcpy.Server 5.0.1 scid=1a2b3c4d log_level=info";
    let (server, fake) = serve(|mut s| async move {
        accept_transport(&mut s, SERIAL, &format!("shell,v2,raw:{COMMAND}")).await;
        send(&mut s, &shell_frame(1, b"[server] INFO: Device")).await;
        send(&mut s, b"\x02\x05\0\0\0oops\n").await;
        send(&mut s, &shell_frame(1, b": SM-G973U\n")).await;
        send(&mut s, b"\x03\x01\0\0\0\x02").await;
    })
    .await;
    let mut shell = server.shell_v2(SERIAL, COMMAND).await.unwrap();
    let mut events = Vec::new();
    while let Some(event) = shell.next().await.unwrap() {
        events.push(event);
    }
    shell.close().await.unwrap();
    fake.await.unwrap();
    assert_eq!(
        events,
        [
            ShellEvent::Stdout(b"[server] INFO: Device".to_vec()),
            ShellEvent::Stderr(b"oops\n".to_vec()),
            ShellEvent::Stdout(b": SM-G973U\n".to_vec()),
            ShellEvent::Exit(2),
        ]
    );
}

#[tokio::test]
async fn shell_v2_cut_mid_frame_is_an_error() {
    let (server, fake) = serve(|mut s| async move {
        accept_transport(&mut s, SERIAL, "shell,v2,raw:true").await;
        send(&mut s, b"\x01\x10\0\0\0only five").await;
    })
    .await;
    let mut shell = server.shell_v2(SERIAL, "true").await.unwrap();
    assert!(matches!(shell.next().await, Err(Error::Protocol(_))));
    fake.await.unwrap();
}

#[tokio::test]
async fn shell_legacy_is_a_raw_stream() {
    let (server, fake) = serve(|mut s| async move {
        accept_transport(&mut s, SERIAL, "shell:getprop ro.product.model").await;
        send(&mut s, b"SM-G973U\n").await;
    })
    .await;
    let mut stream = server
        .shell_legacy(SERIAL, "getprop ro.product.model")
        .await
        .unwrap();
    let mut output = String::new();
    stream.read_to_string(&mut output).await.unwrap();
    fake.await.unwrap();
    assert_eq!(output, "SM-G973U\n");
}

#[tokio::test]
async fn shell_legacy_takes_a_service_of_at_most_4094_bytes() {
    let command = "a".repeat(4094 - "shell:".len());
    let service = format!("shell:{command}");
    let (server, fake) =
        serve(|mut s| async move { accept_transport(&mut s, SERIAL, &service).await }).await;
    server.shell_legacy(SERIAL, &command).await.unwrap();
    fake.await.unwrap();

    // One byte more aborts an adb server before Android 9 on an Android 6 device: refused before
    // connecting.
    let error = unreachable()
        .await
        .shell_legacy(SERIAL, &format!("{command}a"))
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Protocol(_)), "{error:?}");
}

#[tokio::test]
async fn a_long_service_needs_shell_v2() {
    let service = format!("shell,v2,raw:{}", "a".repeat(5000));
    let (server, fake) = serve_connections(|listener| async move {
        let mut s = accept(&listener).await;
        assert_eq!(
            read_request(&mut s).await,
            "host-serial:R58M123ABC:features"
        );
        okay(&mut s).await;
        block(&mut s, "cmd,stat_v2").await;
        listener
    })
    .await;
    let error = server.open(SERIAL, &service).await.unwrap_err();
    assert!(matches!(error, Error::Protocol(_)), "{error:?}");
    // No transport switch, so the service never reached the server.
    assert!(no_more_connections(&fake.await.unwrap()).await);

    let expected = service.clone();
    let (server, fake) = serve_connections(|listener| async move {
        let mut s = accept(&listener).await;
        assert_eq!(
            read_request(&mut s).await,
            "host-serial:R58M123ABC:features"
        );
        okay(&mut s).await;
        block(&mut s, "shell_v2,cmd,stat_v2").await;
        let mut s = accept(&listener).await;
        accept_transport(&mut s, SERIAL, &expected).await;
    })
    .await;
    server.open(SERIAL, &service).await.unwrap();
    fake.await.unwrap();
}

/// A sync frame header: 4-byte id, u32 little-endian length.
async fn read_sync_header(stream: &mut TcpStream) -> ([u8; 4], u32) {
    let mut header = [0; 8];
    stream.read_exact(&mut header).await.unwrap();
    let [a, b, c, d, l0, l1, l2, l3] = header;
    ([a, b, c, d], u32::from_le_bytes([l0, l1, l2, l3]))
}

#[derive(Debug, Default)]
struct Pushed {
    spec: String,
    chunks: Vec<usize>,
    data: Vec<u8>,
    mtime: u32,
    after_done: Vec<u8>,
}

/// Plays adbd's `sync:` side of a push: SEND, DATA... DONE, then `reply`, then whatever follows.
async fn receive_push(mut s: TcpStream, reply: Vec<u8>) -> Pushed {
    accept_transport(&mut s, SERIAL, "sync:").await;
    let (id, len) = read_sync_header(&mut s).await;
    assert_eq!(&id, b"SEND");
    let mut spec = vec![0; len as usize];
    s.read_exact(&mut spec).await.unwrap();
    let mut pushed = Pushed {
        spec: String::from_utf8(spec).unwrap(),
        ..Pushed::default()
    };
    loop {
        match read_sync_header(&mut s).await {
            (id, len) if &id == b"DATA" => {
                let mut chunk = vec![0; len as usize];
                s.read_exact(&mut chunk).await.unwrap();
                pushed.chunks.push(chunk.len());
                pushed.data.extend_from_slice(&chunk);
            }
            (id, mtime) if &id == b"DONE" => {
                pushed.mtime = mtime;
                break;
            }
            (id, _) => panic!("unexpected sync id {:?}", id.escape_ascii().to_string()),
        }
    }
    send(&mut s, &reply).await;
    pushed.after_done = read_to_end(&mut s).await;
    pushed
}

const JAR_PATH: &str = "/data/local/tmp/scrcpy-server.jar";

#[tokio::test]
async fn push_sends_send_data_done_then_quit() {
    let data: Vec<u8> = (0..150 * 1024).map(|i| (i % 251) as u8).collect();
    let (server, fake) = serve(|s| receive_push(s, b"OKAY\0\0\0\0".to_vec())).await;
    server
        .push(SERIAL, &data, JAR_PATH, 0o100644, 1_700_000_000)
        .await
        .unwrap();
    let pushed = fake.await.unwrap();
    assert_eq!(pushed.spec, "/data/local/tmp/scrcpy-server.jar,33188");
    assert_eq!(pushed.chunks, [65536, 65536, 22528]);
    assert!(pushed.data == data, "pushed bytes differ");
    assert_eq!(pushed.mtime, 1_700_000_000);
    assert_eq!(pushed.after_done, b"QUIT\0\0\0\0");
}

#[tokio::test]
async fn push_fail_is_error_fail() {
    let reason = "couldn't create file: Read-only file system";
    let mut reply = b"FAIL".to_vec();
    reply.extend_from_slice(&(reason.len() as u32).to_le_bytes());
    reply.extend_from_slice(reason.as_bytes());
    let (server, fake) = serve(|s| receive_push(s, reply)).await;
    let error = server
        .push(SERIAL, b"#!/bin/sh\n", "/system/bin/x", 0o755, 0)
        .await
        .unwrap_err();
    assert!(matches!(&error, Error::Fail(m) if m == reason), "{error:?}");
    let pushed = fake.await.unwrap();
    assert_eq!(pushed.spec, "/system/bin/x,493");
    assert_eq!(pushed.chunks, [10]);
}

#[tokio::test]
async fn forward_tcp_0_returns_the_allocated_port() {
    let (server, fake) = serve(|mut s| async move {
        let service = accept_switch(&mut s, SERIAL).await;
        okay(&mut s).await;
        okay(&mut s).await;
        block(&mut s, "27183").await;
        service
    })
    .await;
    let port = server.forward(SERIAL, "tcp:0", SOCKET, true).await.unwrap();
    assert_eq!(port, Some(27183));
    assert_eq!(
        fake.await.unwrap(),
        "host-serial:R58M123ABC:forward:norebind:tcp:0;localabstract:scrcpy_1a2b3c4d"
    );
}

#[tokio::test]
async fn forward_rebind_reports_no_port() {
    // Repointing an existing listener sends no port (`install_listener` returns early).
    let (server, fake) = serve(|mut s| async move {
        let service = accept_switch(&mut s, SERIAL).await;
        okay(&mut s).await;
        okay(&mut s).await;
        service
    })
    .await;
    let port = server
        .forward(SERIAL, "tcp:27183", SOCKET, false)
        .await
        .unwrap();
    assert_eq!(port, None);
    assert_eq!(
        fake.await.unwrap(),
        "host-serial:R58M123ABC:forward:tcp:27183;localabstract:scrcpy_1a2b3c4d"
    );
}

#[tokio::test]
async fn forward_fail_is_error_fail() {
    let (server, fake) = serve(|mut s| async move {
        accept_switch(&mut s, SERIAL).await;
        fail(&mut s, "cannot rebind existing socket").await;
    })
    .await;
    let error = server
        .forward(SERIAL, "tcp:27183", SOCKET, true)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, Error::Fail(m) if m == "cannot rebind existing socket"),
        "{error:?}"
    );
    fake.await.unwrap();
}

#[tokio::test]
async fn forward_to_a_missing_device_says_why() {
    // The switch carries the reason; a lone `host-serial:` forward gets an empty FAIL from
    // current servers (`transport_acquirer` in `adb.cpp`).
    let (server, fake) = serve(|mut s| async move {
        assert_eq!(read_request(&mut s).await, "host:transport:x");
        fail(&mut s, "device 'x' not found").await;
        read_to_end(&mut s).await
    })
    .await;
    let error = server
        .forward("x", "tcp:0", SOCKET, true)
        .await
        .unwrap_err();
    assert!(
        matches!(&error, Error::Fail(m) if m == "device 'x' not found"),
        "{error:?}"
    );
    // The forward request is never sent after a failed switch.
    assert_eq!(fake.await.unwrap(), b"");
}

#[tokio::test]
async fn forward_on_a_transport_id() {
    let (server, fake) = serve(|mut s| async move {
        assert_eq!(read_request(&mut s).await, "host:transport-id:11");
        okay(&mut s).await;
        let service = read_request(&mut s).await;
        okay(&mut s).await;
        okay(&mut s).await;
        block(&mut s, "27183").await;
        service
    })
    .await;
    let port = server
        .forward_on(&Target::TransportId(11), "tcp:0", SOCKET, true)
        .await
        .unwrap();
    assert_eq!(port, Some(27183));
    assert_eq!(
        fake.await.unwrap(),
        "host-transport-id:11:forward:norebind:tcp:0;localabstract:scrcpy_1a2b3c4d"
    );
}

#[tokio::test]
async fn kill_forward_reads_okay_okay() {
    let (server, fake) = serve(|mut s| async move {
        let service = accept_switch(&mut s, SERIAL).await;
        okay(&mut s).await;
        okay(&mut s).await;
        service
    })
    .await;
    server.kill_forward(SERIAL, "tcp:27183").await.unwrap();
    assert_eq!(
        fake.await.unwrap(),
        "host-serial:R58M123ABC:killforward:tcp:27183"
    );
}

#[tokio::test]
async fn kill_forward_of_a_missing_listener_is_error_fail() {
    // `handle_forward_request` answers a missing listener with a bare FAIL, no OKAY before it.
    let (server, fake) = serve(|mut s| async move {
        accept_switch(&mut s, SERIAL).await;
        fail(&mut s, "listener 'tcp:27183' not found").await;
    })
    .await;
    let error = server.kill_forward(SERIAL, "tcp:27183").await.unwrap_err();
    assert!(
        matches!(&error, Error::Fail(m) if m == "listener 'tcp:27183' not found"),
        "{error:?}"
    );
    fake.await.unwrap();
}

#[tokio::test]
async fn kill_forward_checks_the_second_status() {
    // Defensive, not AOSP: its killforward never sends OKAY then FAIL. A second status that is not
    // OKAY must still not pass for success.
    let (server, fake) = serve(|mut s| async move {
        accept_switch(&mut s, SERIAL).await;
        okay(&mut s).await;
        fail(&mut s, "listener 'tcp:27183' not found").await;
    })
    .await;
    let error = server.kill_forward(SERIAL, "tcp:27183").await.unwrap_err();
    assert!(
        matches!(&error, Error::Fail(m) if m == "listener 'tcp:27183' not found"),
        "{error:?}"
    );
    fake.await.unwrap();
}

#[tokio::test]
async fn list_forward_parses_rules() {
    let (server, fake) = serve(|mut s| async move {
        assert_eq!(read_request(&mut s).await, "host:list-forward");
        okay(&mut s).await;
        block(
            &mut s,
            "R58M123ABC tcp:27183 localabstract:scrcpy_1a2b3c4d\nemulator-5554 tcp:5000 tcp:6000\n",
        )
        .await;
    })
    .await;
    let rules = server.list_forward().await.unwrap();
    fake.await.unwrap();
    assert_eq!(
        rules,
        [
            ForwardRule {
                serial: SERIAL.into(),
                local: "tcp:27183".into(),
                remote: SOCKET.into(),
            },
            ForwardRule {
                serial: "emulator-5554".into(),
                local: "tcp:5000".into(),
                remote: "tcp:6000".into(),
            },
        ]
    );
}

#[tokio::test]
async fn unreachable_server_is_error_connect() {
    let server = unreachable().await;
    let error = server.version().await.unwrap_err();
    assert!(
        matches!(&error, Error::Connect { addr, .. } if *addr == server.addr()),
        "{error:?}"
    );
}

#[tokio::test]
async fn ensure_server_leaves_a_listening_server_alone() {
    let (server, fake) = serve(|mut s| async move { read_to_end(&mut s).await }).await;
    // Never run: if ensure_server tried to spawn this, it would fail.
    let adb = Path::new("Z:/phonebridge-test/no-such-adb.exe");
    assert!(!ensure_server(adb, &server).await.unwrap());
    // The probe only connects; no request reaches the server.
    assert_eq!(fake.await.unwrap(), b"");
}
