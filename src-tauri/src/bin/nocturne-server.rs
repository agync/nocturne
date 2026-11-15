//! The real app for a browser, as a server: experimental, for the container image. See
//! docs/adr/0012-server-mode.md and `nocturne_app_lib::preview`.
//!
//! ```text
//! nocturne-server            serve until stopped
//! nocturne-server token      print the sign-in link
//! nocturne-server token --rotate
//!                            make a new sign-in token (the old link stops working) and print it
//! nocturne-server health     exit 0 when the server answers on its port
//! ```
//!
//! | Setting | Meaning |
//! |---|---|
//! | `NOCTURNE_DATA_DIR` | The data folder (required). Empty, or one this program made. |
//! | `NOCTURNE_BIND` | The address to listen on (default `127.0.0.1:55066`). |
//! | `NOCTURNE_PUBLIC_ORIGIN` | Where browsers reach it: `https://...`, or `http://` on this computer. |
//! | `NOCTURNE_UI` | The built interface (`ui/dist-server`). |
//! | `NOCTURNE_LEMOND` | The private engine's folder. |

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use nocturne_app_lib::preview::{self, Options, Preview, Token};
use nocturne_app_lib::{DATA_DIR_ENV, logging};
use nocturne_lemonade::online::OwnPort;

const DEFAULT_BIND: &str = "127.0.0.1:55066";
/// The file a server data folder is marked with.
const DATA_MARKER: &str = ".nocturne-server";
/// The sign-in token, owner-only, in the data folder.
const TOKEN_FILE: &str = "server-token";

struct Config {
    data_dir: PathBuf,
    bind: SocketAddr,
    public_origin: Option<String>,
    ui_dir: Option<PathBuf>,
}

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("{message}");
    std::process::exit(2)
}

fn usage() -> ! {
    fail("usage: nocturne-server [token [--rotate] | health]")
}

fn config() -> Config {
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let data_dir = env(DATA_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| fail(format!("set {DATA_DIR_ENV} to the data folder")));
    let bind = env("NOCTURNE_BIND").unwrap_or_else(|| DEFAULT_BIND.into());
    let bind = bind
        .parse()
        .unwrap_or_else(|_| fail(format!("NOCTURNE_BIND is not an address and port: {bind}")));
    let public_origin = env("NOCTURNE_PUBLIC_ORIGIN")
        .map(|o| preview::public_origin(&o).unwrap_or_else(|e| fail(e)));
    Config {
        data_dir,
        bind,
        public_origin,
        ui_dir: env("NOCTURNE_UI").map(PathBuf::from),
    }
}

/// Where a browser opens the interface.
fn origin(config: &Config) -> String {
    config
        .public_origin
        .clone()
        .unwrap_or_else(|| format!("http://127.0.0.1:{}", config.bind.port()))
}

fn sign_in_link(config: &Config, token: &str) -> String {
    format!("{}/#token={token}", origin(config))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let config = config();
    match args.as_slice() {
        [] | ["serve"] => serve(&config),
        ["token"] => token(&config, false),
        ["token", "--rotate"] => token(&config, true),
        ["health"] => std::process::exit(if healthy(config.bind) { 0 } else { 1 }),
        _ => usage(),
    }
}

fn token_path(config: &Config) -> PathBuf {
    config.data_dir.join(TOKEN_FILE)
}

fn token(config: &Config, rotate: bool) {
    let data_dir = preview::claim(&config.data_dir, DATA_MARKER, "nocturne-server")
        .unwrap_or_else(|e| fail(e));
    let path = data_dir.join(TOKEN_FILE);
    let made = if rotate {
        preview::rotate_token_file(&path)
    } else {
        preview::ensure_token_file(&path).map(|(token, _)| token)
    };
    let token = made.unwrap_or_else(|e| fail(format!("the sign-in token cannot be kept: {e}")));
    if rotate {
        println!("A new sign-in link. The old one no longer works.");
    }
    println!("{}", sign_in_link(config, &token));
}

fn serve(config: &Config) {
    let data_dir = preview::claim(&config.data_dir, DATA_MARKER, "nocturne-server")
        .unwrap_or_else(|e| fail(e));
    let (token, fresh) = preview::ensure_token_file(&token_path(config))
        .unwrap_or_else(|e| fail(format!("the sign-in token cannot be kept: {e}")));
    let _log = logging::init(&data_dir.join("logs"));
    let (app, window) = preview::start_app(&data_dir).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let opts = Options {
        token: Token::file(&data_dir.join(TOKEN_FILE)),
        port: config.bind.port(),
        public_origin: config.public_origin.clone(),
        ui_dir: config.ui_dir.clone(),
    };
    let server = Preview::new(app.handle().clone(), window, &data_dir, opts);

    let _own = OwnPort::hold(config.bind.port());
    let listener = tauri::async_runtime::block_on(tokio::net::TcpListener::bind(config.bind))
        .unwrap_or_else(|e| {
            eprintln!("cannot listen on {}: {e}", config.bind);
            std::process::exit(1)
        });
    println!(
        "Nocturne server (experimental), listening on {}",
        config.bind
    );
    if fresh {
        println!(
            "Sign in once with this link: {}",
            sign_in_link(config, &token)
        );
    } else {
        println!("For the sign-in link, run: nocturne-server token");
    }
    preview::serve(app, listener, server);
}

/// The address the health check connects to: the bound one, or loopback when it is bound to
/// every address.
fn check_address(bind: SocketAddr) -> SocketAddr {
    let ip = match bind.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    SocketAddr::new(ip, bind.port())
}

fn healthy(bind: SocketAddr) -> bool {
    let limit = Duration::from_secs(5);
    let Ok(mut stream) = TcpStream::connect_timeout(&check_address(bind), limit) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(limit));
    let _ = stream.set_write_timeout(Some(limit));
    let request = format!(
        "GET /healthz HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
        bind.port()
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut head = [0u8; 12];
    stream.read_exact(&mut head).is_ok() && head.starts_with(b"HTTP/1.1 200")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn the_health_check_reaches_an_address_bound_to_everything_on_loopback() {
        let all: SocketAddr = "0.0.0.0:55066".parse().unwrap();
        assert_eq!(check_address(all), "127.0.0.1:55066".parse().unwrap());
        let all6: SocketAddr = "[::]:55066".parse().unwrap();
        assert_eq!(check_address(all6), "[::1]:55066".parse().unwrap());
        let one: SocketAddr = "192.0.2.7:8080".parse().unwrap();
        assert_eq!(check_address(one), one);
    }

    #[test]
    fn the_link_uses_the_public_origin_when_there_is_one() {
        let mut config = Config {
            data_dir: Path::new("/data").to_path_buf(),
            bind: "0.0.0.0:55066".parse().unwrap(),
            public_origin: None,
            ui_dir: None,
        };
        assert_eq!(
            sign_in_link(&config, "ab"),
            "http://127.0.0.1:55066/#token=ab"
        );
        config.public_origin = Some("https://memory.example.test".into());
        assert_eq!(
            sign_in_link(&config, "ab"),
            "https://memory.example.test/#token=ab"
        );
    }
}
