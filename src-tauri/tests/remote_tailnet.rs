//! Most private on Windows, end to end on any computer: the real transport starts the stand-in
//! nocturne-tailnet with the per-launch key on its input, the stand-in forwards with the key in
//! its header, and the listener admits only what carries it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use nocturne_app_lib::remote::listener::{self, Bind, ListenerConfig};
use nocturne_app_lib::remote::static_files::DirFiles;
use nocturne_app_lib::remote::testing::{
    self,
    client::{Addr, http},
};
use nocturne_helpers::{
    HELPER_KEY_HEADER, HelperDirs, HelperKind, Installed, KeyCarrier, LaunchSpec, Origin,
    TailnetOptions, TailnetTransport, Transport, TransportState,
};
use serde_json::Value;
use sha2_10::{Digest, Sha256};

const KEY: &str = "k7f3a9c2e1b8d4f6a0c3e5b7d9f1a2c4e6b8d0f2a4c6e8b0d2f4a6c8e0b2d4f6";

/// The stand-in helper, built once per run from the helpers crate with its `fake` feature, which
/// a test run of this package does not build by itself.
fn fake_tailnet() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT.get_or_init(build_fake_tailnet).clone()
}

fn build_fake_tailnet() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let profile_dir = exe.parent().and_then(Path::parent).unwrap();
    let profile = profile_dir.file_name().unwrap().to_string_lossy();
    let triple = profile_dir
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| n.contains('-'));
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build = std::process::Command::new(cargo);
    build
        .args(["build", "--locked", "-p", "nocturne-helpers"])
        .args(["--features", "fake", "--bin", "fake-tailnet"])
        .args([
            "--profile",
            if profile == "debug" { "dev" } else { &profile },
        ])
        .arg("--message-format=json-render-diagnostics");
    if let Some(triple) = triple {
        build.args(["--target", &triple]);
    }
    let out = build
        .stderr(std::process::Stdio::inherit())
        .output()
        .expect("cargo runs");
    assert!(out.status.success(), "the stand-in helper is built");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|m| m["reason"] == "compiler-artifact" && m["target"]["name"] == "fake-tailnet")
        .find_map(|m| m["executable"].as_str().map(PathBuf::from))
        .expect("cargo says where the stand-in helper is")
}

fn phone_files(dir: &Path) -> PathBuf {
    let files = dir.join("phone");
    std::fs::create_dir_all(files.join("assets")).unwrap();
    std::fs::write(
        files.join("index.html"),
        b"<!doctype html><title>Nocturne</title>",
    )
    .unwrap();
    std::fs::write(files.join("sw.js"), b"").unwrap();
    std::fs::write(files.join("manifest.webmanifest"), b"{}").unwrap();
    std::fs::write(files.join("assets/app.js"), b"console.log(1)").unwrap();
    files
}

fn installed(dir: &Path) -> Installed {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let path = bin.join("nocturne-tailnet");
    std::fs::copy(fake_tailnet(), &path).unwrap();
    let sha256 = Sha256::digest(std::fs::read(&path).unwrap())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Installed {
        kind: HelperKind::Tailnet,
        version: "fake".into(),
        sha256,
        path,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_key_header_reaches_the_listener_only_through_the_helper() {
    let dir = tempfile::tempdir().unwrap();
    let bench = testing::bench(dir.path());
    let handle = listener::serve(
        ListenerConfig {
            bind: Bind::Tcp {
                key: KEY.into(),
                carrier: KeyCarrier::Header,
            },
            files: Arc::new(DirFiles(phone_files(dir.path()))),
            keepalive: listener::WS_KEEPALIVE,
        },
        bench.core.clone(),
    )
    .await
    .unwrap();
    let Origin::Tcp {
        port: listener_port,
        ..
    } = handle.origin.clone()
    else {
        panic!("a loopback listener")
    };
    let helper = installed(dir.path());
    let t = TailnetTransport::with_options(
        &HelperDirs {
            root: dir.path().join("remote"),
        },
        &helper,
        TailnetOptions {
            stop_wait: Duration::from_secs(5),
            ..TailnetOptions::default()
        },
    );
    let mut rx = t
        .start(LaunchSpec {
            origin: handle.origin.clone(),
            machine_name: "nocturne-ab12".into(),
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(60), async {
        while !matches!(*rx.borrow_and_update(), TransportState::Up { .. }) {
            rx.changed().await.unwrap();
        }
    })
    .await
    .expect("the helper is up");

    let public: u16 = std::fs::read_to_string(dir.path().join("bin/fake-public-port"))
        .unwrap()
        .parse()
        .unwrap();
    let through = Addr::Tcp(public);
    assert_eq!(
        http(&through, "GET", "/p", &[], &[]).await.unwrap().status,
        200
    );
    let forged = http(&through, "GET", "/p", &[(HELPER_KEY_HEADER, "forged")], &[])
        .await
        .unwrap();
    assert_eq!(
        forged.status, 200,
        "the helper's own key replaces a forged one"
    );
    assert!(!String::from_utf8_lossy(&forged.raw).contains(KEY));

    let direct = Addr::Tcp(listener_port);
    assert_eq!(
        http(&direct, "GET", "/p", &[], &[]).await.unwrap().status,
        404
    );
    assert_eq!(
        http(
            &direct,
            "GET",
            "/p",
            &[(HELPER_KEY_HEADER, &KEY[..32])],
            &[]
        )
        .await
        .unwrap()
        .status,
        404
    );
    assert_eq!(
        http(&direct, "GET", "/p", &[("host", KEY)], &[])
            .await
            .unwrap()
            .status,
        404,
        "the key in the Host is not this helper's carrier"
    );

    t.stop().await.unwrap();
    assert_eq!(*rx.borrow(), TransportState::Stopped);
    handle.stop().await;
}
