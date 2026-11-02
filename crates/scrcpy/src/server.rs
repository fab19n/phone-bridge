//! The server command line (`protocol/video.md` §4 and §13 step 5).

use crate::{SERVER_DEVICE_PATH, SERVER_VERSION};

/// Masks a random value to the 31-bit `scid` the server accepts.
pub fn scid_from(random: u32) -> u32 {
    random & 0x7FFF_FFFF
}

/// The abstract socket name for `scid`: `scrcpy_%08x`.
pub fn socket_name(scid: u32) -> String {
    format!("scrcpy_{scid:08x}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Verbose,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// The `log_level` value, as `server.c` `log_level_to_server_string` sends it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Verbose => "verbose",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// A [`ServerOptions`] value that the device shell or the server would reject.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OptionError {
    /// The command line runs through the device shell, so values may not contain the characters
    /// that `server.c` `validate_string` forbids, nor other control characters
    /// (`protocol/video.md` §4.1).
    #[error("server option {key}={value:?} contains a shell-special character")]
    ShellSpecial { key: &'static str, value: String },
    /// `Options.java` parses this option with `Integer.parseInt`, so it must fit an `i32`.
    #[error("server option {key}={value} does not fit a Java int")]
    OutOfRange { key: &'static str, value: u32 },
}

/// The server options PhoneBridge sets. Everything else keeps the server default
/// (`protocol/video.md` §4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerOptions {
    /// 31 bits, from [`scid_from`]: the server parses it with `Integer.parseInt(value, 16)`.
    pub scid: u32,
    pub log_level: LogLevel,
    /// Limits the larger dimension; 0 = unlimited. The server may round it to the encoder's alignment.
    pub max_size: u32,
    /// 0 = unlimited.
    pub max_fps: u32,
    /// Bits per second; 0 = the server default (8 Mbit/s).
    pub video_bit_rate: u32,
    /// Extra `MediaFormat` keys, e.g. `i-frame-interval:int=2` (`protocol/video.md` §11.4).
    pub video_codec_options: Option<String>,
    /// Open the control socket (needed for input, clipboard and `RESET_VIDEO`).
    pub control: bool,
}

impl ServerOptions {
    /// PhoneBridge defaults: H.264, info logging, max 1920 px, 60 fps, 8 Mbit/s, control on.
    pub fn new(scid: u32) -> Self {
        Self {
            scid,
            log_level: LogLevel::Info,
            max_size: 1920,
            max_fps: 60,
            video_bit_rate: 8_000_000,
            video_codec_options: None,
            control: true,
        }
    }

    /// The command to run in a `shell,v2,raw:` session:
    /// `CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 5.0.1 key=value ...`
    ///
    /// Always sends `scid`, `log_level` (the official client always does), `tunnel_forward=true`,
    /// `audio=false` and `video_codec=h264`: H.264 is the server default, but it is the only codec
    /// [`crate::video::VideoStream`] supports. A non-zero `video_bit_rate` is sent even at the
    /// server default, as in `protocol/video.md` §4.2; `max_size`, `max_fps`, `control` and
    /// `video_codec_options` only when they differ from it. The server only warns about unknown
    /// keys, so the tests pin the exact string (§13, B9). A value the device shell would interpret,
    /// or that the server can't parse as an `int`, is an [`OptionError`] (§4.1).
    pub fn command(&self) -> Result<String, OptionError> {
        let mut args = vec![
            format!("CLASSPATH={SERVER_DEVICE_PATH}"),
            "app_process".to_owned(),
            "/".to_owned(),
            "com.genymobile.scrcpy.Server".to_owned(),
            SERVER_VERSION.to_owned(),
            format!("scid={:08x}", java_int("scid", self.scid)?),
            format!("log_level={}", self.log_level.as_str()),
            "tunnel_forward=true".to_owned(),
            "audio=false".to_owned(),
            "video_codec=h264".to_owned(),
        ];
        if self.max_size != 0 {
            args.push(format!("max_size={}", java_int("max_size", self.max_size)?));
        }
        if self.max_fps != 0 {
            args.push(format!("max_fps={}", self.max_fps));
        }
        if self.video_bit_rate != 0 {
            let bit_rate = java_int("video_bit_rate", self.video_bit_rate)?;
            args.push(format!("video_bit_rate={bit_rate}"));
        }
        if !self.control {
            args.push("control=false".to_owned());
        }
        // An empty value parses to "no options" (`CodecOption.parse`), the same as leaving it out.
        if let Some(options) = self
            .video_codec_options
            .as_deref()
            .filter(|o| !o.is_empty())
        {
            if options.chars().any(is_shell_special) {
                return Err(OptionError::ShellSpecial {
                    key: "video_codec_options",
                    value: options.to_owned(),
                });
            }
            args.push(format!("video_codec_options={options}"));
        }
        Ok(args.join(" "))
    }
}

/// What `server.c` `validate_string` rejects, plus every ASCII control character (a tab splits the
/// argument just like a space).
fn is_shell_special(c: char) -> bool {
    c.is_ascii_control() || " ;'\"*$?&`#\\|<>[]{}()!~".contains(c)
}

fn java_int(key: &'static str, value: u32) -> Result<u32, OptionError> {
    if i32::try_from(value).is_ok() {
        Ok(value)
    } else {
        Err(OptionError::OutOfRange { key, value })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREFIX: &str = "CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / \
                          com.genymobile.scrcpy.Server 5.0.1 ";

    #[test]
    fn default_command_golden() {
        // Pins every key: the server ignores unknown keys (e.g. the v3 `send_codec_meta`) silently.
        assert_eq!(
            ServerOptions::new(0x1234abcd).command().unwrap(),
            "CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / \
             com.genymobile.scrcpy.Server 5.0.1 \
             scid=1234abcd log_level=info tunnel_forward=true audio=false video_codec=h264 \
             max_size=1920 max_fps=60 video_bit_rate=8000000"
        );
    }

    #[test]
    fn non_default_command_golden() {
        let options = ServerOptions {
            scid: 0x1a,
            log_level: LogLevel::Debug,
            max_size: 1024,
            max_fps: 30,
            video_bit_rate: 2_000_000,
            video_codec_options: Some("i-frame-interval:int=2".into()),
            control: false,
        };
        assert_eq!(
            options.command().unwrap(),
            format!(
                "{PREFIX}scid=0000001a log_level=debug tunnel_forward=true audio=false \
                 video_codec=h264 max_size=1024 max_fps=30 video_bit_rate=2000000 control=false \
                 video_codec_options=i-frame-interval:int=2"
            )
        );
    }

    #[test]
    fn options_at_the_server_default_are_left_out() {
        let options = ServerOptions {
            max_size: 0,
            max_fps: 0,
            video_bit_rate: 0,
            video_codec_options: Some(String::new()),
            ..ServerOptions::new(0x7fff_ffff)
        };
        assert_eq!(
            options.command().unwrap(),
            format!(
                "{PREFIX}scid=7fffffff log_level=info tunnel_forward=true audio=false \
                 video_codec=h264"
            )
        );
    }

    #[test]
    fn log_levels() {
        for (level, value) in [
            (LogLevel::Verbose, "verbose"),
            (LogLevel::Debug, "debug"),
            (LogLevel::Info, "info"),
            (LogLevel::Warn, "warn"),
            (LogLevel::Error, "error"),
        ] {
            let options = ServerOptions {
                log_level: level,
                ..ServerOptions::new(1)
            };
            assert!(
                options
                    .command()
                    .unwrap()
                    .contains(&format!(" log_level={value} "))
            );
        }
    }

    #[test]
    fn shell_special_characters_are_rejected() {
        for c in " ;'\"*$?&`#\\|<>[]{}()!~\r\n\t\0".chars() {
            let value = format!("i-frame-interval:int=2{c}x");
            let options = ServerOptions {
                video_codec_options: Some(value.clone()),
                ..ServerOptions::new(1)
            };
            assert_eq!(
                options.command(),
                Err(OptionError::ShellSpecial {
                    key: "video_codec_options",
                    value
                }),
                "{c:?}"
            );
        }
    }

    #[test]
    fn values_beyond_a_java_int_are_rejected() {
        let scid = ServerOptions::new(0x8000_0000);
        assert_eq!(
            scid.command(),
            Err(OptionError::OutOfRange {
                key: "scid",
                value: 0x8000_0000
            })
        );
        let bit_rate = ServerOptions {
            video_bit_rate: u32::MAX,
            ..ServerOptions::new(1)
        };
        assert_eq!(
            bit_rate.command(),
            Err(OptionError::OutOfRange {
                key: "video_bit_rate",
                value: u32::MAX
            })
        );
        let max_size = ServerOptions {
            max_size: 1 << 31,
            ..ServerOptions::new(1)
        };
        assert_eq!(
            max_size.command(),
            Err(OptionError::OutOfRange {
                key: "max_size",
                value: 1 << 31
            })
        );
        let max_fps = ServerOptions {
            max_fps: u32::MAX,
            ..ServerOptions::new(1)
        };
        assert!(
            max_fps.command().unwrap().contains(" max_fps=4294967295"),
            "parsed as a float"
        );
    }

    #[test]
    fn scid_and_socket_name() {
        assert_eq!(scid_from(0xffff_ffff), 0x7fff_ffff);
        assert_eq!(scid_from(0x8000_1234), 0x1234);
        assert_eq!(socket_name(0x1a2b3c4d), "scrcpy_1a2b3c4d");
        assert_eq!(socket_name(0x1a), "scrcpy_0000001a");
        assert!(
            ServerOptions::new(0x1a)
                .command()
                .unwrap()
                .contains(" scid=0000001a ")
        );
    }
}
