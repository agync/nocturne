//! A stand-in for `tailscaled` used by tests: owns a state folder and a socket, binds UDP on every
//! interface the way the real one does, and waits.

use nocturne_helpers::HelperKind;
use nocturne_helpers::fake;

fn main() {
    fake::sleeper_if_asked();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let index = fake::record("tailscaled", &args);
    if let Err(e) = fake::check_flags(HelperKind::Tailscaled, "daemon", &args) {
        eprintln!("{e}");
        std::process::exit(2);
    }
    let control = fake::control();
    if fake::hangs(&control) {
        fake::sleep_forever();
    }
    let state = fake::flag_value(&args, "--statedir").map(std::path::PathBuf::from);
    if let Some(state) = &state {
        let _ = std::fs::create_dir_all(state);
        let file = state.join("tailscaled.state");
        if !file.exists() {
            let _ = std::fs::write(file, "{}");
        }
        if control.get("taildrop_files").and_then(|v| v.as_bool()) == Some(true) {
            let _ = std::fs::create_dir_all(state.join("files"));
            let _ = std::fs::write(state.join("files").join("received.partial"), "x");
        }
    }
    #[cfg(unix)]
    let _socket = fake::flag_value(&args, "--socket").and_then(|p| {
        let _ = std::fs::remove_file(&p);
        std::os::unix::net::UnixListener::bind(p).ok()
    });
    let _udp = std::net::UdpSocket::bind("0.0.0.0:0");
    eprintln!("fake-tailscaled: ready");
    fake::behave(&control, index);
    fake::sleep_forever();
}
