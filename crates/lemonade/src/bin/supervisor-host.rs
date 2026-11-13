//! A process that owns a `Supervisor`, so a test can kill it hard and watch the engine follow.
//! Usage: supervisor-host <bundle> <root>. Prints `ready <port> <pid>` once the engine answers.

use std::time::Duration;

use nocturne_lemonade::{Dirs, Supervisor, SupervisorOptions};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [bundle, root] = args.as_slice() else {
        eprintln!("usage: supervisor-host <bundle> <root>");
        std::process::exit(2);
    };
    let sup = match Supervisor::start(Dirs::new(bundle, root), SupervisorOptions::default()).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let port = sup.client().endpoint().map(|e| e.port).unwrap_or(0);
    println!("ready {port} {}", sup.pid().unwrap_or(0));
    loop {
        tokio::time::sleep(Duration::from_secs(3600)).await;
    }
}
