//! A stand-in for `cloudflared` used by tests: a quick tunnel that answers its metrics port, names
//! an address and forwards a loopback "public" port to the origin it was given. With
//! `spoof_metrics` another process answers the metrics port, naming another address.

use std::net::TcpListener;

use nocturne_helpers::HelperKind;
use nocturne_helpers::fake;

fn main() {
    fake::sleeper_if_asked();
    fake::spoofer_if_asked();
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
    let metrics = fake::flag_value(&args, "--metrics");
    let metrics_port = match (
        control.get("spoof_metrics").and_then(|v| v.as_str()),
        control.get("no_metrics").and_then(|v| v.as_bool()) == Some(true),
        metrics,
    ) {
        (Some(other), _, Some(addr)) => fake::spawn_spoofer(&addr, other),
        (None, false, Some(addr)) => fake::serve_metrics(&addr, host.clone()),
        _ => None,
    };
    if let Some(port) = metrics_port {
        eprintln!("INF Starting metrics server on 127.0.0.1:{port}/metrics");
    }
    // As the real one: a quick tunnel needs `--url`, so `--unix-socket` alone is refused, and an
    // ingress rule in the configuration wins over `--url`.
    let url = fake::flag_value(&args, "--url");
    if url.is_none() && fake::flag_value(&args, "--unix-socket").is_some() {
        eprintln!(
            "You did not specify any valid additional argument to the cloudflared tunnel command."
        );
        std::process::exit(1);
    }
    let ingress = fake::flag_value(&args, "--config")
        .and_then(|c| std::fs::read_to_string(c).ok())
        .and_then(|text| fake::ingress_unix(&text));
    let origin = match (
        ingress,
        url.as_deref().and_then(|u| u.strip_prefix("http://")),
    ) {
        (Some(path), _) => Some(fake::Origin::Unix(path.into())),
        (None, Some(addr)) => Some(fake::Origin::Tcp(addr.trim_end_matches('/').to_string())),
        (None, None) => None,
    };
    if let (Some(origin), Ok(listener)) = (origin, TcpListener::bind("127.0.0.1:0")) {
        if let Ok(addr) = listener.local_addr() {
            fake::write_whole(
                &fake::exe_dir().join(fake::PUBLIC_PORT_FILE),
                &addr.port().to_string(),
            );
        }
        let key = std::env::var("TUNNEL_HTTP_HOST_HEADER")
            .ok()
            .map(|k| ("Host".to_string(), k));
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
