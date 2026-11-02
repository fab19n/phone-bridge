//! The adb server's address from the environment, as the AOSP client reads it
//! (`client/commandline.cpp`): `ADB_SERVER_SOCKET` if set, otherwise `ANDROID_ADB_SERVER_ADDRESS`
//! and `ANDROID_ADB_SERVER_PORT`. Only `tcp:` specs are followed; the Windows client cannot use any
//! other kind for the server either (`kLocalSocketTypes` in `socket_spec.cpp`).

use std::env::VarError;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs};

use crate::DEFAULT_PORT;

const SOCKET: &str = "ADB_SERVER_SOCKET";
const ADDRESS: &str = "ANDROID_ADB_SERVER_ADDRESS";
const PORT: &str = "ANDROID_ADB_SERVER_PORT";

/// [`server_addr`] of this process's environment.
pub(crate) fn server_addr_from_env() -> Result<SocketAddr, String> {
    let var = |name: &str| match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(VarError::NotPresent) => Ok(None),
        Err(VarError::NotUnicode(_)) => Err(format!("{name} is not valid Unicode")),
    };
    server_addr(
        var(SOCKET)?.as_deref(),
        var(ADDRESS)?.as_deref(),
        var(PORT)?.as_deref(),
    )
}

/// The address the three variables' values name, in the client's order. An empty value counts as
/// unset.
pub(crate) fn server_addr(
    socket: Option<&str>,
    address: Option<&str>,
    port: Option<&str>,
) -> Result<SocketAddr, String> {
    if let Some(spec) = socket.filter(|spec| !spec.is_empty()) {
        return tcp_spec_addr(spec).map_err(|error| format!("{SOCKET} \"{spec}\": {error}"));
    }
    let port = match port.filter(|port| !port.is_empty()) {
        None => DEFAULT_PORT,
        Some(text) => parse_int(text)
            .and_then(port_number)
            .ok_or_else(|| format!("{PORT} \"{text}\" is not a port from 1 to 65535"))?,
    };
    match address.filter(|host| !host.is_empty()) {
        None => Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, port))),
        // The client joins the two into `tcp:<host>:<port>`.
        Some(host) => tcp_spec_addr(&format!("tcp:{host}:{port}"))
            .map_err(|error| format!("{ADDRESS} \"{host}\": {error}")),
    }
}

/// `tcp:<port>`, `tcp:<host>:<port>` or `tcp:[<ipv6>]:<port>` (`parse_tcp_socket_spec` in
/// `socket_spec.cpp`). No host, or `localhost`, is IPv4 loopback, the only address the Windows
/// server listens on (`tcp_host_is_local`, `network_loopback_server`). Any other name is resolved
/// here, which blocks, and its first address is used, as `network_connect` does.
///
/// Stricter than AOSP where AOSP's guess is no server: a host without a port (which it takes as
/// 5555, the adbd port) and a port with trailing junk are errors.
fn tcp_spec_addr(spec: &str) -> Result<SocketAddr, String> {
    let rest = spec
        .strip_prefix("tcp:")
        .ok_or("only tcp: socket specs are supported")?;
    if let Some(port) = parse_int(rest) {
        let port = port_number(port).ok_or("the port must be from 1 to 65535")?;
        return Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
    }
    let (host, port) = match rest.strip_prefix('[') {
        Some(bracketed) => bracketed.split_once("]:"),
        None => rest.split_once(':').filter(|(_, port)| !port.contains(':')),
    }
    .ok_or("expected <port>, <host>:<port> or [<ipv6>]:<port>")?;
    let port = parse_int(port)
        .and_then(port_number)
        .ok_or("the port must be from 1 to 65535")?;
    if host.is_empty() {
        return Err("no host".into());
    }
    if host == "localhost" {
        return Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, port)));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    (host, port)
        .to_socket_addrs()
        .map_err(|error| format!("cannot resolve {host}: {error}"))?
        .next()
        .ok_or_else(|| format!("{host} has no address"))
}

/// `android::base::ParseInt`: leading whitespace, then a decimal integer with an optional sign or
/// `0x` and hex digits, and nothing after it.
fn parse_int(text: &str) -> Option<i64> {
    let text = text.trim_start_matches([' ', '\t', '\n', '\x0b', '\x0c', '\r']);
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) if hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
            i64::from_str_radix(hex, 16).ok()
        }
        Some(_) => None,
        None => text.parse().ok(),
    }
}

/// A port a client can connect to.
fn port_number(n: i64) -> Option<u16> {
    u16::try_from(n).ok().filter(|&port| port != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }

    fn loopback(port: u16) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, port))
    }

    #[test]
    fn nothing_set_is_loopback_5037() {
        assert_eq!(server_addr(None, None, None).unwrap(), loopback(5037));
        assert_eq!(
            server_addr(Some(""), Some(""), Some("")).unwrap(),
            loopback(5037)
        );
    }

    #[test]
    fn port_is_read_like_parse_int() {
        // `set ANDROID_ADB_SERVER_PORT= 5038` in cmd.exe keeps the space; the client accepts it.
        let cases = [
            ("5038", 5038),
            (" 5038", 5038),
            ("\t+5038", 5038),
            ("0x13ae", 5038),
            ("0X13AE", 5038),
            ("1", 1),
            ("65535", 65535),
        ];
        for (text, port) in cases {
            assert_eq!(
                server_addr(None, None, Some(text)).unwrap(),
                loopback(port),
                "{text:?}"
            );
        }
        let bad = [
            "0",
            "65536",
            "-1",
            "abc",
            "5038 ",
            "50 38",
            "0x",
            "-0x10",
            "0x-1",
            "99999999999999999999",
        ];
        for text in bad {
            let error = server_addr(None, None, Some(text)).unwrap_err();
            assert!(error.starts_with(PORT), "{text:?}: {error}");
        }
    }

    #[test]
    fn address_and_port_are_joined_into_a_tcp_spec() {
        assert_eq!(
            server_addr(None, Some("192.168.1.5"), Some("5038")).unwrap(),
            addr("192.168.1.5:5038")
        );
        assert_eq!(
            server_addr(None, Some("localhost"), None).unwrap(),
            loopback(5037)
        );
        assert_eq!(
            server_addr(None, Some("[::1]"), Some("5038")).unwrap(),
            addr("[::1]:5038")
        );
        // `tcp:::1:5037` is no address the client can use either.
        let error = server_addr(None, Some("::1"), None).unwrap_err();
        assert!(error.starts_with(ADDRESS), "{error}");
    }

    #[test]
    fn server_socket_wins_over_address_and_port() {
        let cases = [
            ("tcp:5038", loopback(5038)),
            ("tcp: 5039", loopback(5039)),
            ("tcp:localhost:5040", loopback(5040)),
            ("tcp:127.0.0.2:5041", addr("127.0.0.2:5041")),
            ("tcp:[::1]:5042", addr("[::1]:5042")),
        ];
        for (spec, expected) in cases {
            assert_eq!(
                server_addr(Some(spec), Some("192.168.1.5"), Some("6000")).unwrap(),
                expected,
                "{spec}"
            );
        }
    }

    #[test]
    fn server_sockets_without_a_usable_tcp_address_are_errors() {
        let bad = [
            "localfilesystem:/tmp/adb.sock",
            "localabstract:adb",
            "vsock:3:5037",
            "5037",
            "tcp:",
            "tcp:0",
            "tcp:65536",
            "tcp:localhost",
            "tcp::5037",
            "tcp:localhost:0",
            "tcp:localhost:5037x",
            "tcp:::1",
            "tcp:[::1]",
            "tcp:a:b:5037",
        ];
        for spec in bad {
            let error = server_addr(Some(spec), None, None).unwrap_err();
            assert!(error.starts_with(SOCKET), "{spec}: {error}");
        }
    }
}
