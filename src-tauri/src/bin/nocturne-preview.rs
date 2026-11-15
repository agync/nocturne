//! The real app in a browser, for development only. See `nocturne_app_lib::preview`.
//!
//! ```text
//! nocturne-preview --data-dir DIR [--port 55066] [--public-origin https://host:8443] [--ui DIR]
//! ```
//!
//! The data folder must be empty or one this tool made, never the app's own. The engine is found
//! the way the app finds it (`NOCTURNE_LEMOND`, then the checkout's `vendor/lemonade`).

use std::path::PathBuf;

use nocturne_app_lib::logging;
use nocturne_app_lib::preview::{self, DATA_MARKER, MARKER, Options, Preview};
use nocturne_lemonade::online::OwnPort;

struct Args {
    data_dir: PathBuf,
    port: u16,
    public_origin: Option<String>,
    ui_dir: Option<PathBuf>,
}

fn usage() -> ! {
    eprintln!("usage: nocturne-preview --data-dir DIR [--port N] [--public-origin URL] [--ui DIR]");
    std::process::exit(2)
}

fn parse() -> Args {
    let mut args = std::env::args().skip(1);
    let (mut data_dir, mut port, mut public_origin, mut ui_dir) = (None, 55066u16, None, None);
    while let Some(flag) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| usage());
        match flag.as_str() {
            "--data-dir" => data_dir = Some(PathBuf::from(value())),
            "--port" => port = value().parse().unwrap_or_else(|_| usage()),
            "--public-origin" => public_origin = Some(value()),
            "--ui" => ui_dir = Some(PathBuf::from(value())),
            _ => usage(),
        }
    }
    Args {
        data_dir: data_dir.unwrap_or_else(|| usage()),
        port,
        public_origin,
        ui_dir,
    }
}

fn main() {
    let args = parse();
    let data_dir = preview::claim(&args.data_dir, DATA_MARKER, MARKER).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    });
    let _log = logging::init(&data_dir.join("logs"));
    let (app, window) = preview::start_app(&data_dir).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });

    let token = preview::new_token();
    let opts = Options {
        token: token.as_str().into(),
        port: args.port,
        public_origin: args.public_origin.clone(),
        ui_dir: args.ui_dir.clone(),
    };
    let server = Preview::new(app.handle().clone(), window, &data_dir, opts);

    let _own = OwnPort::hold(args.port);
    let listener =
        tauri::async_runtime::block_on(tokio::net::TcpListener::bind(("127.0.0.1", args.port)))
            .unwrap_or_else(|e| {
                eprintln!("cannot listen on 127.0.0.1:{}: {e}", args.port);
                std::process::exit(1)
            });
    println!("{MARKER}: development only, never shipped");
    println!("listening on http://127.0.0.1:{}", args.port);
    println!("open http://127.0.0.1:{}/#token={token}", args.port);
    if let Some(public) = &args.public_origin {
        println!("or {}/#token={token}", public.trim_end_matches('/'));
    }
    preview::serve(app, listener, server);
}
