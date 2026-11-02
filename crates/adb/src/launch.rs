//! Starting an adb server when none is listening: `adb -P <port> start-server` (REVIEW B10).

use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;
use tokio::time::{Instant, sleep, timeout};

use crate::{Error, Result};

/// `CREATE_NO_WINDOW`: no console window for the console program adb (REVIEW B9).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// `start-server` returns once the new server acks, which it does after its device scan (at most
/// about 3 s, `adb_wait_for_device_initialization` in `adb.cpp`). This leaves room for a slow disk
/// or a virus scan of a fresh `adb.exe`.
const START_TIMEOUT: Duration = Duration::from_secs(30);
/// How long to wait for the port to accept connections after `start-server` succeeded.
const LISTEN_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// How much of `start-server`'s stderr to keep for an error message.
const STDERR_MAX: u64 = 16 * 1024;
/// How long to keep reading stderr after `start-server` exits.
const STDERR_GRACE: Duration = Duration::from_millis(500);

/// `env_error` is why `addr` is only a guess (see [`crate::AdbServer::from_env`]).
pub(crate) async fn ensure_server(
    adb_exe: &Path,
    addr: SocketAddr,
    env_error: Option<&str>,
) -> Result<bool> {
    let refused = match TcpStream::connect(addr).await {
        Ok(_) => return Ok(false),
        Err(error) => error,
    };
    if let Some(reason) = env_error {
        return Err(Error::Env(format!(
            "{reason}; not starting an adb server at {addr}"
        )));
    }
    // `adb -P <port> start-server` listens on 127.0.0.1 only. For any other address, even another
    // loopback one such as [::1] or 127.0.0.2, the new server would never answer at `addr`.
    if addr.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) {
        return Err(Error::Connect {
            addr,
            source: refused,
        });
    }
    tracing::info!(adb = %adb_exe.display(), %addr, "no adb server is listening; starting one");
    let (status, stderr) = timeout(START_TIMEOUT, start_server(adb_exe, addr.port()))
        .await
        .map_err(|_| Error::Connect {
            addr,
            source: io::Error::new(io::ErrorKind::TimedOut, "`adb start-server` timed out"),
        })??;
    if !status.success() {
        return Err(Error::Connect {
            addr,
            source: io::Error::other(format!("`adb start-server` failed ({status}): {stderr}")),
        });
    }
    let deadline = Instant::now() + LISTEN_TIMEOUT;
    loop {
        match TcpStream::connect(addr).await {
            Ok(_) => return Ok(true),
            Err(source) if Instant::now() >= deadline => {
                return Err(Error::Connect { addr, source });
            }
            Err(_) => sleep(POLL_INTERVAL).await,
        }
    }
}

/// Runs `adb -P <port> start-server` to completion and returns its status and stderr.
async fn start_server(adb_exe: &Path, port: u16) -> Result<(ExitStatus, String)> {
    let mut command = std::process::Command::new(adb_exe);
    command
        .arg("-P")
        .arg(port.to_string())
        .arg("start-server")
        // Only `-P` decides where the new server listens, so it binds the address polled next.
        .env_remove("ADB_SERVER_SOCKET")
        .env_remove("ANDROID_ADB_SERVER_ADDRESS")
        .env_remove("ANDROID_ADB_SERVER_PORT")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = tokio::process::Command::from(command)
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("cannot run {}: {error}", adb_exe.display()),
            )
        })?;

    let mut pipe = child.stderr.take();
    let mut stderr = Vec::new();
    let status = {
        let read = async {
            if let Some(pipe) = pipe.as_mut() {
                let mut head = pipe.take(STDERR_MAX);
                let _ = head.read_to_end(&mut stderr).await;
                // Keep draining past the cap so adb never blocks on a full pipe.
                let _ = tokio::io::copy(head.into_inner(), &mut tokio::io::sink()).await;
            }
        };
        tokio::pin!(read);
        let mut read_done = false;
        let status = loop {
            tokio::select! {
                status = child.wait() => break status?,
                () = &mut read, if !read_done => read_done = true,
            }
        };
        // The server that start-server spawns could inherit the pipe and keep it open (adb.cpp
        // guards against that on Windows), so do not wait for EOF for long.
        if !read_done {
            let _ = timeout(STDERR_GRACE, &mut read).await;
        }
        status
    };
    let stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
    tracing::debug!(%status, stderr, "adb start-server finished");
    Ok((status, stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port 9 (discard) has no listener here, and this adb binary doesn't exist, so a test can tell
    // "refused before spawning" (Error::Connect) from "tried to spawn" (Error::Io).
    const NO_SUCH_ADB: &str = "Z:/no/such/dir/adb.exe";

    #[tokio::test]
    async fn starts_no_server_for_other_loopback_addresses() {
        for addr in ["[::1]:9", "127.0.0.2:9"] {
            let error = ensure_server(Path::new(NO_SUCH_ADB), addr.parse().unwrap(), None)
                .await
                .unwrap_err();
            assert!(matches!(error, Error::Connect { .. }), "{addr}: {error}");
        }
    }

    #[tokio::test]
    async fn tries_to_start_a_server_for_127_0_0_1() {
        let error = ensure_server(Path::new(NO_SUCH_ADB), "127.0.0.1:9".parse().unwrap(), None)
            .await
            .unwrap_err();
        assert!(
            matches!(&error, Error::Io(e) if e.kind() == io::ErrorKind::NotFound),
            "{error}"
        );
    }
}
