//! Client for the adb server's host protocol on TCP 5037.
//!
//! PhoneBridge talks to the running adb server directly instead of running the `adb` CLI, so an adb
//! client whose version differs from the server can never restart it (REVIEW B10, test 0G). The
//! connection sequence that uses this crate is `protocol/video.md` §13.
//!
//! References (AOSP `packages/modules/adb`): `SERVICES.TXT` (host and device services),
//! `protocol.txt`, `SYNC.TXT` (file push) and `shell_protocol.h` (shell v2 framing). The `.TXT`
//! files now live in `docs/dev/` (`services.md`, `overview.md` for the request framing, `sync.md`);
//! the reply formats here were checked against `adb.cpp`, `sockets.cpp`, `transport.cpp` and
//! `client/adb_client.cpp` at commit `1cf2f017`.

mod env;
mod launch;
mod shell;
mod sync;
mod wire;

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// The adb server's default port; `ADB_SERVER_SOCKET` or `ANDROID_ADB_SERVER_PORT` override it.
pub const DEFAULT_PORT: u16 = 5037;

/// The longest device service every server can pass to a device that takes 4096-byte packets, as
/// devices before Android 7 do (`MAX_PAYLOAD` in their `adb.h`). The server sends the service plus
/// a NUL and aborts, instead of failing the request, if that exceeds `max_payload - 1` (servers
/// from before Android 9) or `max_payload` (later ones; `connect_to_remote` in `sockets.cpp`).
/// Devices with `shell_v2` (Android 7 and later) take 256 KiB (`MAX_PAYLOAD_V2`).
const SERVICE_MAX_V1: usize = 4094;

/// What the server lists for a device whose serial is empty (`append_transport` in
/// `transport.cpp`). No request by serial matches it (`atransport::MatchesTarget`).
const NO_SERIAL: &str = "(no serial number)";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("adb server not reachable at {addr}: {source}")]
    Connect {
        addr: SocketAddr,
        source: std::io::Error,
    },
    #[error("adb I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The server (or device) answered `FAIL` with this message.
    #[error("adb: {0}")]
    Fail(String),
    #[error("adb protocol error: {0}")]
    Protocol(String),
    /// The adb environment variables name no server address this crate can use.
    #[error("adb server address from the environment: {0}")]
    Env(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A device as listed by `host:devices-l`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// `(no serial number)` for a device without one, which only its transport id reaches (see
    /// [`Device::target`]).
    pub serial: String,
    pub state: DeviceState,
    pub product: Option<String>,
    pub model: Option<String>,
    pub device: Option<String>,
    pub transport_id: Option<u64>,
}

impl Device {
    /// The device by its serial, or by its transport id if it has no serial.
    pub fn target(&self) -> Target {
        match self.transport_id {
            Some(id) if self.serial == NO_SERIAL => Target::TransportId(id),
            _ => Target::Serial(self.serial.clone()),
        }
    }
}

/// The device a request is for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// A serial as `host:devices-l` lists it.
    Serial(String),
    /// A [`Device::transport_id`] (`host:transport-id:`, `host-transport-id:`; `adb.cpp`,
    /// `sockets.cpp`). It also reaches a device without a serial, and only the connection it was
    /// listed for: a reconnected device gets a new id.
    TransportId(u64),
}

impl Target {
    /// Switches a connection to the device; answered with a bare `OKAY`.
    fn transport_request(&self) -> String {
        match self {
            Self::Serial(serial) => format!("host:transport:{serial}"),
            Self::TransportId(id) => format!("host:transport-id:{id}"),
        }
    }

    /// `service` as a host request for the device.
    fn host_request(&self, service: &str) -> String {
        match self {
            Self::Serial(serial) => format!("host-serial:{serial}:{service}"),
            Self::TransportId(id) => format!("host-transport-id:{id}:{service}"),
        }
    }
}

/// A device's connection state, as the server reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceState {
    /// Connected and authorized.
    Device,
    /// The phone hasn't accepted this PC's RSA key yet ("Allow USB debugging?").
    Unauthorized,
    Offline,
    Authorizing,
    Connecting,
    /// The host OS denies access to the USB device: `no permissions (<reason>); see [<url>]`.
    NoPermissions,
    Recovery,
    Sideload,
    Bootloader,
    Host,
    /// Any other state, e.g. `rescue` or `detached`.
    Unknown(String),
}

/// A forward rule from `host:list-forward`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForwardRule {
    pub serial: String,
    pub local: String,
    pub remote: String,
}

/// One event from a shell v2 session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    /// The process exited with this code.
    Exit(u8),
}

/// A `shell,v2,raw:` session: stdout, stderr and the exit code arrive as separate frames.
#[derive(Debug)]
pub struct ShellV2 {
    stream: TcpStream,
    decoder: shell::Decoder,
}

impl ShellV2 {
    /// The next event, or `None` once the server closes the stream.
    ///
    /// Output frames longer than 64 KiB arrive as several events. Cancel-safe: bytes read by a
    /// call that is dropped (in `select!`, say) are kept for the next one.
    pub async fn next(&mut self) -> Result<Option<ShellEvent>> {
        loop {
            if let Some(event) = self.decoder.decode()? {
                return Ok(Some(event));
            }
            if self.stream.read_buf(&mut self.decoder.buf).await? == 0 {
                return if self.decoder.is_idle() {
                    Ok(None)
                } else {
                    Err(Error::Protocol(
                        "connection closed in the middle of a shell frame".into(),
                    ))
                };
            }
        }
    }

    /// Ends the session by closing the connection.
    pub async fn close(mut self) -> Result<()> {
        shutdown(&mut self.stream).await
    }
}

/// A `host:track-devices` subscription: the server sends the full device list on every change.
#[derive(Debug)]
pub struct DeviceTracker {
    stream: TcpStream,
    buf: Vec<u8>,
}

impl DeviceTracker {
    /// The next device list as `(serial, state)` pairs, or `None` once the server closes the stream.
    ///
    /// The first list arrives right after subscribing. Cancel-safe, like [`ShellV2::next`]. A
    /// device without a serial shows as `(no serial number)`; [`AdbServer::devices`] has its
    /// transport id.
    pub async fn next(&mut self) -> Result<Option<Vec<(String, DeviceState)>>> {
        loop {
            if let Some(list) = wire::take_block(&mut self.buf)? {
                return wire::parse_device_list(&String::from_utf8_lossy(&list)).map(Some);
            }
            if self.stream.read_buf(&mut self.buf).await? == 0 {
                return if self.buf.is_empty() {
                    Ok(None)
                } else {
                    Err(Error::Protocol(
                        "connection closed in the middle of a device list".into(),
                    ))
                };
            }
        }
    }
}

/// Handle to an adb server. Every call opens its own connection, as the protocol expects.
#[derive(Clone, Debug)]
pub struct AdbServer {
    addr: SocketAddr,
    /// Why [`AdbServer::from_env`] fell back to the default address, if it did.
    env_error: Option<String>,
}

impl AdbServer {
    pub fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            env_error: None,
        }
    }

    /// The server the AOSP client would use (`client/commandline.cpp`): `ADB_SERVER_SOCKET`
    /// (`tcp:<port>`, `tcp:<host>:<port>`) if set, otherwise `ANDROID_ADB_SERVER_ADDRESS` (default
    /// `127.0.0.1`) on `ANDROID_ADB_SERVER_PORT` (default [`DEFAULT_PORT`]). An empty variable
    /// counts as unset, and a host name other than `localhost` is resolved here, which blocks.
    ///
    /// A value [`AdbServer::try_from_env`] rejects is logged and `127.0.0.1:5037` used instead;
    /// [`ensure_server`] never starts a server at that guessed address.
    pub fn from_env() -> Self {
        Self::from_resolved(env::server_addr_from_env())
    }

    /// Like [`AdbServer::from_env`], but a value the AOSP client would reject, or a socket spec
    /// other than `tcp:` (which the Windows client cannot use either), is an [`Error::Env`].
    pub fn try_from_env() -> Result<Self> {
        env::server_addr_from_env()
            .map(Self::new)
            .map_err(Error::Env)
    }

    fn from_resolved(resolved: Result<SocketAddr, String>) -> Self {
        match resolved {
            Ok(addr) => Self::new(addr),
            Err(error) => {
                let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_PORT));
                tracing::warn!(%error, %addr, "unusable adb server address in the environment");
                Self {
                    addr,
                    env_error: Some(error),
                }
            }
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// `host:version`: the server's protocol version (41 for current platform-tools).
    pub async fn version(&self) -> Result<u32> {
        wire::parse_version(&self.query("host:version").await?)
    }

    /// `host:devices-l`.
    pub async fn devices(&self) -> Result<Vec<Device>> {
        wire::parse_devices_l(&self.query("host:devices-l").await?)
    }

    /// `host:track-devices`.
    pub async fn track_devices(&self) -> Result<DeviceTracker> {
        Ok(DeviceTracker {
            stream: self.host("host:track-devices").await?,
            buf: Vec::new(),
        })
    }

    /// `host-serial:<serial>:features`, e.g. `shell_v2`, `cmd`, `stat_v2`.
    pub async fn features(&self, serial: &str) -> Result<Vec<String>> {
        self.features_on(&Target::Serial(serial.to_owned())).await
    }

    /// [`AdbServer::features`] for any [`Target`].
    pub async fn features_on(&self, target: &Target) -> Result<Vec<String>> {
        let reply = self.query(&target.host_request("features")).await?;
        Ok(wire::parse_features(&reply))
    }

    /// `host:transport:<serial>`, then the device service `service` (e.g. `localabstract:scrcpy_1234abcd`).
    /// After `OKAY` the returned stream is a raw byte pipe to that service.
    ///
    /// The transport switch answers a bare `OKAY` (`host:tport:` would add a transport id). A
    /// device service that is not there yet, such as a socket nobody listens on, fails with
    /// `closed`. The stream has `TCP_NODELAY` set, so small writes such as scrcpy control messages
    /// go out at once.
    ///
    /// A service longer than 4094 bytes goes only to a device with the `shell_v2` feature, which
    /// costs a `features` query first. Older devices take 4096-byte packets, and the adb server
    /// aborts, instead of failing the request, on a service that does not fit (REVIEW B10).
    pub async fn open(&self, serial: &str, service: &str) -> Result<TcpStream> {
        self.open_on(&Target::Serial(serial.to_owned()), service)
            .await
    }

    /// [`AdbServer::open`] for any [`Target`].
    pub async fn open_on(&self, target: &Target, service: &str) -> Result<TcpStream> {
        if service.len() > SERVICE_MAX_V1
            && !self
                .features_on(target)
                .await?
                .iter()
                .any(|feature| feature == "shell_v2")
        {
            return Err(Error::Protocol(format!(
                "device service of {} bytes; a device without shell_v2 accepts {SERVICE_MAX_V1}",
                service.len()
            )));
        }
        let mut stream = self.host(&target.transport_request()).await?;
        wire::request(&mut stream, service).await?;
        Ok(stream)
    }

    /// `shell,v2,raw:<command>`. Needs the device feature `shell_v2`.
    pub async fn shell_v2(&self, serial: &str, command: &str) -> Result<ShellV2> {
        self.shell_v2_on(&Target::Serial(serial.to_owned()), command)
            .await
    }

    /// [`AdbServer::shell_v2`] for any [`Target`].
    pub async fn shell_v2_on(&self, target: &Target, command: &str) -> Result<ShellV2> {
        Ok(ShellV2 {
            stream: self
                .open_on(target, &format!("shell,v2,raw:{command}"))
                .await?,
            decoder: shell::Decoder::default(),
        })
    }

    /// Legacy `shell:<command>`: stdout and stderr arrive merged, with no exit code.
    ///
    /// `shell:<command>` may be at most 4094 bytes, what every device without `shell_v2` accepts
    /// (see [`AdbServer::open`]). The AOSP client's own check (`RemoteShell` in
    /// `client/commandline.cpp`) lets 4096 through.
    pub async fn shell_legacy(&self, serial: &str, command: &str) -> Result<TcpStream> {
        self.shell_legacy_on(&Target::Serial(serial.to_owned()), command)
            .await
    }

    /// [`AdbServer::shell_legacy`] for any [`Target`].
    pub async fn shell_legacy_on(&self, target: &Target, command: &str) -> Result<TcpStream> {
        let service = format!("shell:{command}");
        if service.len() > SERVICE_MAX_V1 {
            return Err(Error::Protocol(format!(
                "legacy shell command of {} bytes; old devices accept {SERVICE_MAX_V1}",
                service.len()
            )));
        }
        self.open_on(target, &service).await
    }

    /// `sync:` SEND/DATA/DONE: writes `data` to `remote_path` with `mode` (e.g. `0o644`) and `mtime`.
    ///
    /// `mtime` is in seconds since the Unix epoch. `remote_path,mode` may be at most 1024 bytes.
    pub async fn push(
        &self,
        serial: &str,
        data: &[u8],
        remote_path: &str,
        mode: u32,
        mtime: u32,
    ) -> Result<()> {
        self.push_on(
            &Target::Serial(serial.to_owned()),
            data,
            remote_path,
            mode,
            mtime,
        )
        .await
    }

    /// [`AdbServer::push`] for any [`Target`].
    pub async fn push_on(
        &self,
        target: &Target,
        data: &[u8],
        remote_path: &str,
        mode: u32,
        mtime: u32,
    ) -> Result<()> {
        let send = sync::send_request(remote_path, mode)?;
        let mut stream = self.open_on(target, "sync:").await?;
        sync::push(&mut stream, &send, data, mtime).await
    }

    /// `host:transport:<serial>`, then `host-serial:<serial>:forward[:norebind]:<local>;<remote>`
    /// on the same connection. Returns the port the server chose when `local` is `tcp:0`.
    ///
    /// The switch fails with the server's reason for a missing, offline or unauthorized device,
    /// where current servers answer a lone `host-serial:` forward with an empty `FAIL`
    /// (`transport_acquirer` in `adb.cpp`). Servers before Android 11 ignore the switch for a
    /// forward, hence the serial again.
    ///
    /// The server answers `OKAY` twice, then sends the port of every newly bound `tcp:` listener,
    /// so this is also `Some` for `tcp:<port>`; it is `None` when an existing rule was repointed.
    pub async fn forward(
        &self,
        serial: &str,
        local: &str,
        remote: &str,
        norebind: bool,
    ) -> Result<Option<u16>> {
        self.forward_on(&Target::Serial(serial.to_owned()), local, remote, norebind)
            .await
    }

    /// [`AdbServer::forward`] for any [`Target`].
    pub async fn forward_on(
        &self,
        target: &Target,
        local: &str,
        remote: &str,
        norebind: bool,
    ) -> Result<Option<u16>> {
        let norebind = if norebind { "norebind:" } else { "" };
        let service = target.host_request(&format!("forward:{norebind}{local};{remote}"));
        let mut stream = self.host(&target.transport_request()).await?;
        wire::request(&mut stream, &service).await?;
        wire::read_status(&mut stream).await?;
        match wire::read_block_or_eof(&mut stream, "the forward port").await? {
            Some(port) if !port.is_empty() => {
                wire::parse_port(&String::from_utf8_lossy(&port)).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// `host:transport:<serial>`, then `host-serial:<serial>:killforward:<local>`, as in
    /// [`AdbServer::forward`]. A missing listener is a bare `FAIL`
    /// (`listener '<local>' not found`). The server removes the listener on `local` whichever
    /// device it forwards to (`remove_listener` in `adb_listeners.cpp`).
    pub async fn kill_forward(&self, serial: &str, local: &str) -> Result<()> {
        self.kill_forward_on(&Target::Serial(serial.to_owned()), local)
            .await
    }

    /// [`AdbServer::kill_forward`] for any [`Target`].
    pub async fn kill_forward_on(&self, target: &Target, local: &str) -> Result<()> {
        let service = target.host_request(&format!("killforward:{local}"));
        let mut stream = self.host(&target.transport_request()).await?;
        wire::request(&mut stream, &service).await?;
        wire::read_status(&mut stream).await
    }

    /// `host:list-forward`.
    pub async fn list_forward(&self) -> Result<Vec<ForwardRule>> {
        wire::parse_forwards(&self.query("host:list-forward").await?)
    }

    async fn connect(&self) -> Result<TcpStream> {
        let stream = TcpStream::connect(self.addr)
            .await
            .map_err(|source| Error::Connect {
                addr: self.addr,
                source,
            })?;
        stream.set_nodelay(true)?;
        Ok(stream)
    }

    /// A new connection on which `service` has been answered `OKAY`.
    async fn host(&self, service: &str) -> Result<TcpStream> {
        let mut stream = self.connect().await?;
        wire::request(&mut stream, service).await?;
        Ok(stream)
    }

    /// A host service that answers `OKAY` plus one length-prefixed reply.
    async fn query(&self, service: &str) -> Result<String> {
        let mut stream = self.host(service).await?;
        let reply = wire::read_block(&mut stream, "the reply").await?;
        Ok(String::from_utf8_lossy(&reply).into_owned())
    }
}

/// Starts an adb server with `adb_exe start-server` if none is listening at `server`, with no
/// console window on Windows. Returns `true` if it had to start one. This is the only time
/// PhoneBridge runs an adb binary (REVIEW B10).
///
/// "Listening" means the port accepts a TCP connection; nothing is sent on it. The command is
/// `<adb_exe> -P <port> start-server`, run with the server-address variables removed, and only
/// when `server` is 127.0.0.1: that's the only address `-P` binds, so for any other address
/// (even another loopback one) this returns the connect error instead. Never at the address
/// [`AdbServer::from_env`] fell back to: the user's tools would not look there, and a second
/// server would fight theirs over the devices ([`Error::Env`]).
pub async fn ensure_server(adb_exe: &Path, server: &AdbServer) -> Result<bool> {
    launch::ensure_server(adb_exe, server.addr, server.env_error.as_deref()).await
}

/// Shuts down the write half; the caller then drops the stream. A peer that already left is fine.
async fn shutdown(stream: &mut TcpStream) -> Result<()> {
    match stream.shutdown().await {
        Err(error)
            if !matches!(
                error.kind(),
                io::ErrorKind::NotConnected
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::ConnectionAborted
                    | io::ErrorKind::BrokenPipe
            ) =>
        {
            Err(error.into())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_name_their_device() {
        let serial = Target::Serial("R58M123ABC".into());
        assert_eq!(serial.transport_request(), "host:transport:R58M123ABC");
        assert_eq!(
            serial.host_request("features"),
            "host-serial:R58M123ABC:features"
        );
        let id = Target::TransportId(11);
        assert_eq!(id.transport_request(), "host:transport-id:11");
        assert_eq!(id.host_request("features"), "host-transport-id:11:features");
    }

    #[test]
    fn a_device_without_a_serial_is_targeted_by_transport_id() {
        let device = Device {
            serial: NO_SERIAL.into(),
            state: DeviceState::Device,
            product: None,
            model: None,
            device: None,
            transport_id: Some(11),
        };
        assert_eq!(device.target(), Target::TransportId(11));
        let serial = Device {
            serial: "R58M123ABC".into(),
            ..device.clone()
        };
        assert_eq!(serial.target(), Target::Serial("R58M123ABC".into()));
        // Servers before transport ids: the placeholder is all there is.
        let old = Device {
            transport_id: None,
            ..device
        };
        assert_eq!(old.target(), Target::Serial(NO_SERIAL.into()));
    }

    #[test]
    fn an_unusable_environment_falls_back_to_the_default_address() {
        let guessed = AdbServer::from_resolved(Err("ADB_SERVER_SOCKET \"x\": nope".into()));
        assert_eq!(
            guessed.addr(),
            SocketAddr::from((Ipv4Addr::LOCALHOST, DEFAULT_PORT))
        );
        assert_eq!(
            guessed.env_error.as_deref(),
            Some("ADB_SERVER_SOCKET \"x\": nope")
        );
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, 5038));
        let server = AdbServer::from_resolved(Ok(addr));
        assert_eq!(server.addr(), addr);
        assert_eq!(server.env_error, None);
    }

    #[test]
    fn from_env_agrees_with_try_from_env() {
        let server = AdbServer::from_env();
        match AdbServer::try_from_env() {
            Ok(strict) => assert_eq!(server.addr(), strict.addr()),
            Err(_) => assert!(server.env_error.is_some()),
        }
    }

    #[tokio::test]
    async fn no_server_is_started_at_a_guessed_address() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        let server = AdbServer {
            addr,
            env_error: Some("ADB_SERVER_SOCKET \"localfilesystem:/x\": unsupported".into()),
        };
        // Never run: starting it would fail with an I/O error instead.
        let adb = Path::new("Z:/phonebridge-test/no-such-adb.exe");
        let error = ensure_server(adb, &server).await.unwrap_err();
        assert!(
            matches!(&error, Error::Env(m) if m.contains("localfilesystem")),
            "{error:?}"
        );
    }
}
