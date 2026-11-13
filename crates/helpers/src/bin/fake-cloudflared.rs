//! A stand-in for `cloudflared` used by tests: a quick tunnel that answers its metrics port, names
//! an address and forwards a loopback "public" port to the origin it was given.

use std::net::TcpListener;

use nocturne_helpers::HelperKind;
use nocturne_helpers::fake;

fn main() {
    fake::sleeper_if_asked();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let index = fake::record("cloudflared", &args);
    if args.first().map(String::as_str) != Some("tunnel") {
        eprintln!("fake-cloudflared: expected the tunnel command");
        std::process::exit(2);
    }
    if let Err(e) = fake::check_flags(HelperKind::Cloudflared, "tunnel", &args[1..]) {
        eprintln!("{e}");
        std::process::exit(2);
    }
    let control = fake::control();
    if fake::hangs(&control) {
        fake::sleep_forever();
    }
    let host = control.get("address").and_then(|v| v.as_str()).map_or_else(
        || format!("fake-{index}-{}.trycloudflare.com", std::process::id()),
        String::from,
    );
    if control.get("no_metrics").and_then(|v| v.as_bool()) != Some(true)
        && let Some(addr) = fake::flag_value(&args, "--metrics")
    {
        fake::serve_metrics(&addr, host.clone());
    }
    let origin = match (
        fake::flag_value(&args, "--unix-socket"),
        fake::flag_value(&args, "--url"),
    ) {
        (Some(path), _) => Some(fake::Origin::Unix(path.into())),
        (None, Some(url)) => Some(fake::Origin::Tcp(
            url.trim_start_matches("http://")
                .trim_end_matches('/')
                .to_string(),
        )),
        _ => None,
    };
    if let (Some(origin), Ok(listener)) = (origin, TcpListener::bind("127.0.0.1:0")) {
        if let Ok(addr) = listener.local_addr() {
            let _ = std::fs::write(
                fake::exe_dir().join(fake::PUBLIC_PORT_FILE),
                addr.port().to_string(),
            );
        }
        let key = std::env::var("TUNNEL_HTTP_HOST_HEADER").ok();
        let record = control
            .get("record_relay")
            .and_then(|v| v.as_str())
            .map(std::path::PathBuf::from);
        std::thread::spawn(move || fake::relay(listener, origin, key, record));
    }
    eprintln!(
        "INF Your quick Tunnel has been created! Visit it at (it may take some time to be reachable):"
    );
    eprintln!("INF |  https://{host}  |");
    eprintln!("INF Registered tunnel connection connIndex=0");
    fake::behave(&control, index);
    fake::sleep_forever();
}
