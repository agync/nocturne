//! Download, check and install a pinned helper. Nothing here runs until `install` is called, and
//! `install` is only called after the person agreed to the download.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::lock::{HelpersLock, is_unpinned, platform, safe_member};
use crate::{HelperDirs, HelperError, HelperKind, InstallProgress, Installed};

/// The most a single extracted binary may take, so a hostile archive cannot fill the disk.
const MAX_MEMBER_BYTES: u64 = 512 * 1024 * 1024;

/// What to fetch and what it must hash to.
#[derive(Debug, Clone)]
pub(crate) struct Source {
    pub kind: HelperKind,
    pub version: String,
    pub url: String,
    pub file: String,
    pub sha256: String,
    pub member: Option<String>,
    pub member_sha256: Option<String>,
    /// The download's exact size: nothing larger is accepted or kept.
    pub max_bytes: u64,
    /// Hosts a redirect may lead to (https only).
    pub hosts: Vec<String>,
}

pub(crate) fn kind_dir(kind: HelperKind) -> &'static str {
    kind.name()
}

pub(crate) fn binary_name(kind: HelperKind) -> String {
    let name = match kind {
        HelperKind::Cloudflared => "cloudflared",
        HelperKind::Tailnet => "nocturne-tailnet",
    };
    format!("{name}{}", if cfg!(windows) { ".exe" } else { "" })
}

pub(crate) fn helpers_dir(dirs: &HelperDirs) -> PathBuf {
    dirs.root.join("helpers")
}

pub(crate) fn install_dir(dirs: &HelperDirs, kind: HelperKind, version: &str) -> PathBuf {
    helpers_dir(dirs).join(kind_dir(kind)).join(version)
}

fn part_path(dirs: &HelperDirs, kind: HelperKind, version: &str) -> PathBuf {
    helpers_dir(dirs).join(format!(".{}-{version}.part", kind_dir(kind)))
}

/// The helper of this kind already installed, with the checksum it was pinned at.
pub(crate) fn installed_with(
    lock: &HelpersLock,
    kind: HelperKind,
    dirs: &HelperDirs,
) -> Option<Installed> {
    let helper = lock.helper(kind)?;
    let asset = helper.assets.get(platform()?)?;
    let path = install_dir(dirs, kind, &helper.version).join(binary_name(kind));
    path.is_file().then(|| Installed {
        kind,
        version: helper.version.clone(),
        sha256: asset
            .member_sha256
            .clone()
            .unwrap_or_else(|| asset.sha256.clone()),
        path,
    })
}

/// The hosts a download may come from: the one in the lock, and the ones the lock names for
/// redirects.
fn allowed_hosts(
    helper: &crate::lock::LockedHelper,
    asset: &crate::lock::LockedAsset,
) -> Vec<String> {
    let base = asset.base_url.as_deref().unwrap_or(&helper.base_url);
    let mut hosts: Vec<String> = helper.redirect_hosts.clone();
    if let Some(h) = base
        .strip_prefix("https://")
        .and_then(|r| r.split('/').next())
    {
        hosts.push(h.to_string());
    }
    hosts
}

pub(crate) fn source_for(lock: &HelpersLock, kind: HelperKind) -> Result<Source, HelperError> {
    let helper = lock.helper(kind).ok_or(HelperError::Unsupported)?;
    let asset = platform()
        .and_then(|p| helper.assets.get(p))
        .ok_or(HelperError::Unsupported)?;
    if helper.placeholder || is_unpinned(&asset.sha256) {
        return Err(HelperError::Unsupported);
    }
    Ok(Source {
        kind,
        version: helper.version.clone(),
        url: helper.asset_url(asset),
        file: asset.file.clone(),
        sha256: asset.sha256.clone(),
        member: asset.member.clone(),
        member_sha256: asset.member_sha256.clone(),
        max_bytes: asset.size,
        hosts: allowed_hosts(helper, asset),
    })
}

pub(crate) async fn install_embedded(
    kind: HelperKind,
    dirs: &HelperDirs,
    progress: watch::Sender<InstallProgress>,
    cancel: CancellationToken,
) -> Result<Installed, HelperError> {
    let source = source_for(&HelpersLock::current(), kind)?;
    if !source.url.starts_with("https://") {
        return Err(HelperError::Unsupported);
    }
    let client = client(false, &source.hosts)?;
    install_source(&source, dirs, progress, cancel, &client).await
}

pub(crate) fn client(no_proxy: bool, hosts: &[String]) -> Result<reqwest::Client, HelperError> {
    let hosts = hosts.to_vec();
    let policy = reqwest::redirect::Policy::custom(move |attempt| {
        let url = attempt.url();
        let ok = url.scheme() == "https"
            && url.host_str().is_some_and(|h| hosts.iter().any(|a| a == h))
            && attempt.previous().len() < 5;
        if ok {
            attempt.follow()
        } else {
            attempt.error("a redirect that is not allowed")
        }
    });
    let mut builder = reqwest::Client::builder()
        .redirect(policy)
        .user_agent("nocturne-helper-download")
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60));
    if no_proxy {
        builder = builder.no_proxy();
    }
    builder
        .build()
        .map_err(|e| HelperError::Download(e.to_string()))
}

/// Delete a paused download. A paused install keeps its part so the next try resumes; this is
/// for when the person cancels instead.
pub async fn remove_partial(kind: HelperKind, dirs: &HelperDirs) {
    let dir = helpers_dir(dirs);
    let prefix = format!(".{}-", kind_dir(kind));
    let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) && name.ends_with(".part") {
            let _ = tokio::fs::remove_file(entry.path()).await;
        }
    }
}

pub(crate) async fn install_source(
    source: &Source,
    dirs: &HelperDirs,
    progress: watch::Sender<InstallProgress>,
    cancel: CancellationToken,
    client: &reqwest::Client,
) -> Result<Installed, HelperError> {
    if let Some(m) = &source.member
        && !safe_member(m)
    {
        return Err(HelperError::Mismatch);
    }
    let dir = install_dir(dirs, source.kind, &source.version);
    let helpers = helpers_dir(dirs);
    {
        let root = dirs.root.clone();
        let helpers = helpers.clone();
        spawn_blocking(move || {
            make_private_dir(&root)?;
            make_private_dir(&helpers)
        })
        .await?;
    }
    let part = part_path(dirs, source.kind, &source.version);
    download(source, &part, &progress, &cancel, client).await?;

    let part_hash = part.clone();
    let got = spawn_blocking(move || hash_file(&part_hash)).await?;
    if got != source.sha256 {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(HelperError::Mismatch);
    }

    let staging = helpers.join(format!(
        ".{}-{}.staging-{}",
        kind_dir(source.kind),
        source.version,
        std::process::id()
    ));
    let job = (
        source.clone(),
        part.clone(),
        staging.clone(),
        dir.clone(),
        helpers.join(kind_dir(source.kind)),
    );
    let result = spawn_blocking(move || {
        let (source, part, staging, dir, kind_root) = job;
        let outcome = extract_and_place(&source, &part, &staging, &dir, &kind_root);
        if outcome.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        outcome
    })
    .await;
    let _ = tokio::fs::remove_file(&part).await;
    result?;

    Ok(Installed {
        kind: source.kind,
        version: source.version.clone(),
        sha256: source
            .member_sha256
            .clone()
            .unwrap_or_else(|| source.sha256.clone()),
        path: dir.join(binary_name(source.kind)),
    })
}

async fn spawn_blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, HelperError> + Send + 'static,
) -> Result<T, HelperError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| HelperError::Download(e.to_string()))?
}

fn net_error(e: reqwest::Error) -> HelperError {
    if e.is_redirect() {
        HelperError::Download("the server redirected somewhere that is not allowed".into())
    } else if e.is_connect() || e.is_timeout() {
        HelperError::NoInternet
    } else {
        HelperError::Download(e.to_string())
    }
}

fn disk_error(e: std::io::Error) -> HelperError {
    HelperError::Download(format!("could not write the file: {e}"))
}

/// Fetch into `part`, resuming with a Range request when a part is already there.
async fn download(
    source: &Source,
    part: &Path,
    progress: &watch::Sender<InstallProgress>,
    cancel: &CancellationToken,
    client: &reqwest::Client,
) -> Result<(), HelperError> {
    let have = tokio::fs::metadata(part)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    let mut request = client.get(&source.url);
    if have > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let mut response = tokio::select! {
        r = request.send() => r.map_err(net_error)?,
        () = cancel.cancelled() => return Err(HelperError::Stopped),
    };
    let status = response.status();
    let mut offset = 0;
    let append = match status.as_u16() {
        206 => {
            let starts = response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.starts_with(&format!("bytes {have}-")));
            if !starts {
                let _ = tokio::fs::remove_file(part).await;
                return Err(HelperError::Download(
                    "the server answered with the wrong part".into(),
                ));
            }
            offset = have;
            true
        }
        200 => false,
        416 if have > 0 => return Ok(()),
        code => {
            return Err(HelperError::Download(format!("the server answered {code}")));
        }
    };
    let total = response.content_length().map_or(0, |n| n + offset);
    if total > source.max_bytes {
        return Err(HelperError::Download(
            "the download is larger than expected".into(),
        ));
    }
    let mut file = open_part(part, append).await?;
    let mut received = offset;
    progress.send_replace(InstallProgress { received, total });
    loop {
        let chunk = tokio::select! {
            c = response.chunk() => c.map_err(net_error)?,
            () = cancel.cancelled() => {
                let _ = file.flush().await;
                return Err(HelperError::Stopped);
            }
        };
        let Some(chunk) = chunk else { break };
        file.write_all(&chunk).await.map_err(disk_error)?;
        received += chunk.len() as u64;
        if received > source.max_bytes {
            drop(file);
            let _ = tokio::fs::remove_file(part).await;
            return Err(HelperError::Download(
                "the download is larger than expected".into(),
            ));
        }
        progress.send_replace(InstallProgress { received, total });
    }
    file.flush().await.map_err(disk_error)?;
    if total > 0 && received != total {
        return Err(HelperError::Download("the download ended early".into()));
    }
    Ok(())
}

async fn open_part(path: &Path, append: bool) -> Result<tokio::fs::File, HelperError> {
    let mut options = tokio::fs::OpenOptions::new();
    options.create(true).write(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    #[cfg(unix)]
    options.mode(0o600);
    options.open(path).await.map_err(disk_error)
}

fn hash_file(path: &Path) -> Result<String, HelperError> {
    let mut file = std::fs::File::open(path).map_err(disk_error)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf).map_err(disk_error)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Create a folder only the owner can open.
pub(crate) fn make_private_dir(path: &Path) -> Result<(), HelperError> {
    std::fs::create_dir_all(path).map_err(disk_error)?;
    restrict(path, 0o700)
}

#[cfg(unix)]
fn restrict(path: &Path, mode: u32) -> Result<(), HelperError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(disk_error)
}

#[cfg(not(unix))]
fn restrict(_path: &Path, _mode: u32) -> Result<(), HelperError> {
    Ok(())
}

fn extract_and_place(
    source: &Source,
    part: &Path,
    staging: &Path,
    dir: &Path,
    kind_root: &Path,
) -> Result<(), HelperError> {
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging).map_err(disk_error)?;
    restrict(staging, 0o700)?;
    let main = staging.join(binary_name(source.kind));
    match &source.member {
        None => {
            std::fs::copy(part, &main).map_err(disk_error)?;
        }
        Some(member) => {
            let wanted: Vec<(&str, PathBuf)> = vec![(member.as_str(), main.clone())];
            if source.file.ends_with(".zip") {
                extract_zip(part, &wanted)?;
            } else {
                extract_tgz(part, &wanted)?;
            }
        }
    }
    restrict(&main, 0o700)?;
    if hash_file(&main)?
        != source
            .member_sha256
            .clone()
            .unwrap_or(source.sha256.clone())
    {
        return Err(HelperError::Mismatch);
    }
    std::fs::create_dir_all(kind_root).map_err(disk_error)?;
    restrict(kind_root, 0o700)?;
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(staging, dir).map_err(disk_error)
}

fn missing(member: &str) -> HelperError {
    HelperError::Download(format!("the download does not hold {member}"))
}

/// Pick archive members by exact name; nothing else is written, so a name with `..` or a
/// leading `/` has nowhere to go.
fn extract_tgz(part: &Path, wanted: &[(&str, PathBuf)]) -> Result<(), HelperError> {
    let file = std::fs::File::open(part).map_err(disk_error)?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let mut found = vec![false; wanted.len()];
    for entry in archive.entries().map_err(disk_error)? {
        let mut entry = entry.map_err(disk_error)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let name = entry.path_bytes().into_owned();
        let name = String::from_utf8_lossy(&name);
        let name = name.strip_prefix("./").unwrap_or(&name);
        if let Some(i) = wanted.iter().position(|(m, _)| *m == name) {
            write_member(&mut entry, &wanted[i].1)?;
            found[i] = true;
        }
    }
    match wanted.iter().zip(&found).find(|(_, f)| !**f) {
        Some(((m, _), _)) => Err(missing(m)),
        None => Ok(()),
    }
}

fn extract_zip(part: &Path, wanted: &[(&str, PathBuf)]) -> Result<(), HelperError> {
    let file = std::fs::File::open(part).map_err(disk_error)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| HelperError::Download(e.to_string()))?;
    for (member, dest) in wanted {
        let mut entry = archive.by_name(member).map_err(|_| missing(member))?;
        write_member(&mut entry, dest)?;
    }
    Ok(())
}

fn write_member(from: &mut impl Read, to: &Path) -> Result<(), HelperError> {
    let mut out = std::fs::File::create(to).map_err(disk_error)?;
    let copied =
        std::io::copy(&mut from.take(MAX_MEMBER_BYTES + 1), &mut out).map_err(disk_error)?;
    out.flush().map_err(disk_error)?;
    if copied > MAX_MEMBER_BYTES {
        return Err(HelperError::Download("the file is too large".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    pub(crate) struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new(label: &str) -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/helpers-test-tmp")
                .join(format!(
                    "{label}-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn dirs(&self) -> HelperDirs {
            HelperDirs {
                root: self.0.join("remote"),
            }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct Server {
        port: u16,
        hits: Arc<AtomicUsize>,
        ranges: Arc<std::sync::Mutex<Vec<Option<String>>>>,
    }

    /// Serves `body` on loopback with Range support. With `stall_at`, the first request sends that
    /// many bytes and then goes quiet, like a connection that stops.
    async fn serve(body: Vec<u8>, stall_at: Option<usize>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let ranges = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (h, r) = (hits.clone(), ranges.clone());
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let n = h.fetch_add(1, Ordering::SeqCst);
                let body = body.clone();
                let r = r.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut byte = [0u8; 1];
                    while !head.ends_with(b"\r\n\r\n") {
                        if stream.read(&mut byte).await.unwrap_or(0) == 0 {
                            return;
                        }
                        head.push(byte[0]);
                    }
                    let head = String::from_utf8_lossy(&head).into_owned();
                    let range = head
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("range: bytes=")
                                .map(String::from)
                        })
                        .map(|v| v.trim_end_matches('-').to_string());
                    r.lock().unwrap().push(range.clone());
                    let start: usize = range.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0);
                    let part = &body[start..];
                    let header = if range.is_some() {
                        format!(
                            "HTTP/1.1 206 Partial Content\r\ncontent-range: bytes {start}-{}/{}\r\ncontent-length: {}\r\n\r\n",
                            body.len() - 1,
                            body.len(),
                            part.len()
                        )
                    } else {
                        format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n", part.len())
                    };
                    let _ = stream.write_all(header.as_bytes()).await;
                    match stall_at {
                        Some(at) if n == 0 => {
                            let _ = stream.write_all(&part[..at]).await;
                            let _ = stream.flush().await;
                            tokio::time::sleep(Duration::from_secs(30)).await;
                        }
                        _ => {
                            let _ = stream.write_all(part).await;
                        }
                    }
                });
            }
        });
        Server { port, hits, ranges }
    }

    fn sha(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    fn raw_source(server: &Server, body: &[u8]) -> Source {
        Source {
            kind: HelperKind::Cloudflared,
            version: "9.9.9".into(),
            url: format!("http://127.0.0.1:{}/cloudflared-bin", server.port),
            file: "cloudflared-bin".into(),
            sha256: sha(body),
            member: None,
            member_sha256: None,
            max_bytes: 1 << 20,
            hosts: Vec::new(),
        }
    }

    fn tgz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, data) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_entry_type(tar::EntryType::Regular);
            // Written raw, so a name with `..` can be put in on purpose.
            let raw = &mut header.as_old_mut().name;
            raw.fill(0);
            raw[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            builder.append(&header, *data).unwrap();
        }
        let tar_bytes = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar_bytes).unwrap();
        gz.finish().unwrap()
    }

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in entries {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    async fn run(
        source: &Source,
        dirs: &HelperDirs,
    ) -> (
        Result<Installed, HelperError>,
        watch::Receiver<InstallProgress>,
    ) {
        let (tx, rx) = watch::channel(InstallProgress::default());
        let client = client(true, &source.hosts).unwrap();
        let result = install_source(source, dirs, tx, CancellationToken::new(), &client).await;
        (result, rx)
    }

    #[tokio::test]
    async fn installs_owner_only() {
        let tmp = TempDir::new("owner");
        let body = b"#!/bin/sh\nexit 0\n".to_vec();
        let server = serve(body.clone(), None).await;
        let dirs = tmp.dirs();
        let (result, progress) = run(&raw_source(&server, &body), &dirs).await;
        let installed = result.unwrap();
        assert_eq!(installed.sha256, sha(&body));
        assert_eq!(std::fs::read(&installed.path).unwrap(), body);
        assert_eq!(progress.borrow().received, body.len() as u64);
        assert_eq!(verify_ok(&installed), Ok(()));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&installed.path), 0o700);
            for dir in [
                dirs.root.clone(),
                helpers_dir(&dirs),
                helpers_dir(&dirs).join("cloudflared"),
                installed.path.parent().unwrap().to_path_buf(),
            ] {
                assert_eq!(mode(&dir), 0o700, "{}", dir.display());
            }
        }
        let leftovers: Vec<_> = std::fs::read_dir(helpers_dir(&dirs))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    fn verify_ok(i: &Installed) -> Result<(), HelperError> {
        crate::verify(i)
    }

    #[tokio::test]
    async fn verify_catches_a_changed_binary() {
        let tmp = TempDir::new("changed");
        let body = b"original".to_vec();
        let server = serve(body.clone(), None).await;
        let (result, _) = run(&raw_source(&server, &body), &tmp.dirs()).await;
        let installed = result.unwrap();
        std::fs::write(&installed.path, b"swapped").unwrap();
        assert_eq!(verify_ok(&installed), Err(HelperError::Mismatch));
    }

    #[tokio::test]
    async fn mismatch_removes_the_file() {
        let tmp = TempDir::new("mismatch");
        let server = serve(b"not the pinned bytes".to_vec(), None).await;
        let mut source = raw_source(&server, b"the pinned bytes");
        source.sha256 = sha(b"the pinned bytes");
        let dirs = tmp.dirs();
        let (result, _) = run(&source, &dirs).await;
        assert_eq!(result, Err(HelperError::Mismatch));
        assert!(!part_path(&dirs, HelperKind::Cloudflared, "9.9.9").exists());
        assert!(!install_dir(&dirs, HelperKind::Cloudflared, "9.9.9").exists());
    }

    #[tokio::test]
    async fn pause_then_resume_uses_range() {
        let tmp = TempDir::new("resume");
        let body: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
        let server = serve(body.clone(), Some(8_000)).await;
        let source = raw_source(&server, &body);
        let dirs = tmp.dirs();
        let client = client(true, &source.hosts).unwrap();

        let (tx, mut rx) = watch::channel(InstallProgress::default());
        let cancel = CancellationToken::new();
        let stopper = cancel.clone();
        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                if rx.borrow().received >= 8_000 {
                    stopper.cancel();
                    return;
                }
            }
        });
        let first = install_source(&source, &dirs, tx, cancel, &client).await;
        assert_eq!(first, Err(HelperError::Stopped));
        let part = part_path(&dirs, HelperKind::Cloudflared, "9.9.9");
        let kept = std::fs::metadata(&part).unwrap().len();
        assert!(kept > 0 && kept < body.len() as u64, "{kept}");

        let (second, progress) = run(&source, &dirs).await;
        assert_eq!(std::fs::read(second.unwrap().path).unwrap(), body);
        assert_eq!(progress.borrow().received, body.len() as u64);
        let ranges = server.ranges.lock().unwrap().clone();
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0], None);
        assert_eq!(ranges[1], Some(kept.to_string()));
    }

    #[tokio::test]
    async fn cancel_removes_the_part() {
        let tmp = TempDir::new("cancel");
        let body = vec![7u8; 10_000];
        let server = serve(body.clone(), Some(4_000)).await;
        let source = raw_source(&server, &body);
        let dirs = tmp.dirs();
        let client = client(true, &source.hosts).unwrap();
        let (tx, mut rx) = watch::channel(InstallProgress::default());
        let cancel = CancellationToken::new();
        let stopper = cancel.clone();
        tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                if rx.borrow().received >= 4_000 {
                    stopper.cancel();
                    return;
                }
            }
        });
        assert_eq!(
            install_source(&source, &dirs, tx, cancel, &client).await,
            Err(HelperError::Stopped)
        );
        let part = part_path(&dirs, HelperKind::Cloudflared, "9.9.9");
        assert!(part.exists());
        remove_partial(HelperKind::Cloudflared, &dirs).await;
        assert!(!part.exists());
    }

    #[tokio::test]
    async fn no_internet_is_its_own_error() {
        let tmp = TempDir::new("offline");
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let source = Source {
            kind: HelperKind::Cloudflared,
            version: "9.9.9".into(),
            url: format!("http://127.0.0.1:{port}/x"),
            file: "x".into(),
            sha256: sha(b"x"),
            member: None,
            member_sha256: None,
            max_bytes: 1 << 20,
            hosts: Vec::new(),
        };
        let (result, _) = run(&source, &tmp.dirs()).await;
        assert_eq!(result, Err(HelperError::NoInternet));
    }

    #[tokio::test]
    async fn rejects_archive_path_traversal() {
        let tmp = TempDir::new("traversal");
        let archive = tgz(&[
            ("../escape", b"evil"),
            ("/abs", b"evil"),
            ("pkg/other", b"x"),
        ]);
        let server = serve(archive.clone(), None).await;
        let mut source = raw_source(&server, &archive);
        source.file = "pkg.tgz".into();
        source.member = Some("pkg/cloudflared".into());
        source.member_sha256 = Some(sha(b"evil"));
        let dirs = tmp.dirs();
        let (result, _) = run(&source, &dirs).await;
        assert!(
            matches!(result, Err(HelperError::Download(_))),
            "{result:?}"
        );
        assert!(!tmp.0.join("escape").exists());
        assert!(!tmp.0.join("remote/helpers/escape").exists());
        assert!(!install_dir(&dirs, HelperKind::Cloudflared, "9.9.9").exists());

        let mut bad = source.clone();
        bad.member = Some("../escape".into());
        let (result, _) = run(&bad, &dirs).await;
        assert_eq!(result, Err(HelperError::Mismatch));
    }

    #[tokio::test]
    async fn archives_install_their_pinned_members() {
        let tmp = TempDir::new("archives");
        let helper = b"helper bytes".to_vec();
        for (zip, name) in [(false, "pkg.tgz"), (true, "pkg.zip")] {
            let entries: [(&str, &[u8]); 3] = [
                ("pkg/readme", b"hello"),
                ("pkg/cloudflared", &helper),
                ("pkg/other", b"other"),
            ];
            let archive = if zip { zip_of(&entries) } else { tgz(&entries) };
            let server = serve(archive.clone(), None).await;
            let mut source = raw_source(&server, &archive);
            source.file = name.into();
            source.member = Some("pkg/cloudflared".into());
            source.member_sha256 = Some(sha(&helper));
            let dirs = tmp.dirs();
            let (result, _) = run(&source, &dirs).await;
            let installed = result.unwrap();
            assert_eq!(installed.sha256, sha(&helper));
            assert_eq!(std::fs::read(&installed.path).unwrap(), helper);
            let dir = installed.path.parent().unwrap();
            assert!(!dir.join("other").exists() && !dir.join("readme").exists());

            let mut wrong = source.clone();
            wrong.member_sha256 = Some(sha(b"other"));
            let (result, _) = run(&wrong, &dirs).await;
            assert_eq!(result, Err(HelperError::Mismatch));
        }
    }

    #[tokio::test]
    async fn nothing_downloads_without_install_call() {
        let tmp = TempDir::new("quiet");
        let server = serve(b"x".to_vec(), None).await;
        let dirs = tmp.dirs();
        let lock = HelpersLock::current();
        assert!(installed_with(&lock, HelperKind::Cloudflared, &dirs).is_none());
        assert!(installed_with(&lock, HelperKind::Tailnet, &dirs).is_none());
        let pinned = Installed {
            kind: HelperKind::Cloudflared,
            version: "1".into(),
            sha256: sha(b"x"),
            path: tmp.0.join("none"),
        };
        let transport = crate::transport_for(HelperKind::Cloudflared, &dirs, &pinned);
        let _ = (transport.kind(), transport.info());
        let _ = crate::verify(&pinned);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(server.hits.load(Ordering::SeqCst), 0);
        assert!(!dirs.root.exists(), "no folder is made before consent");
    }

    #[test]
    fn unpinned_helpers_are_not_downloaded() {
        let lock = HelpersLock::parse(HelpersLock::embedded_text()).unwrap();
        assert_eq!(
            source_for(&lock, HelperKind::Tailnet).err(),
            Some(HelperError::Unsupported)
        );
    }

    #[test]
    fn an_installed_helper_carries_its_pin() {
        let tmp = TempDir::new("pin");
        let dirs = tmp.dirs();
        let lock = HelpersLock::parse(HelpersLock::embedded_text()).unwrap();
        let version = lock.helper(HelperKind::Tailnet).unwrap().version.clone();
        let dir = install_dir(&dirs, HelperKind::Tailnet, &version);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(binary_name(HelperKind::Tailnet)), b"x").unwrap();
        let found = installed_with(&lock, HelperKind::Tailnet, &dirs);
        if platform().is_some() {
            let found = found.unwrap();
            assert_eq!(found.version, version);
            assert_eq!(
                found.sha256,
                lock.helper(HelperKind::Tailnet).unwrap().assets[platform().unwrap()].sha256
            );
            assert_eq!(crate::verify(&found), Err(HelperError::Mismatch));
        }
    }

    /// Answers every request with exactly these bytes, then closes.
    async fn serve_raw(response: Vec<u8>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let response = response.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut byte = [0u8; 1];
                    while !head.ends_with(b"\r\n\r\n") {
                        if stream.read(&mut byte).await.unwrap_or(0) == 0 {
                            return;
                        }
                        head.push(byte[0]);
                    }
                    let _ = stream.write_all(&response).await;
                });
            }
        });
        port
    }

    fn capped(port: u16, max_bytes: u64) -> Source {
        Source {
            kind: HelperKind::Cloudflared,
            version: "9.9.9".into(),
            url: format!("http://127.0.0.1:{port}/x"),
            file: "x".into(),
            sha256: sha(b"x"),
            member: None,
            member_sha256: None,
            max_bytes,
            hosts: vec!["127.0.0.1".into()],
        }
    }

    #[tokio::test]
    async fn a_body_without_a_length_cannot_outgrow_the_pin() {
        let tmp = TempDir::new("nolen");
        let mut response = b"HTTP/1.0 200 OK\r\n\r\n".to_vec();
        response.extend(vec![1u8; 100_000]);
        let source = capped(serve_raw(response).await, 10_000);
        let dirs = tmp.dirs();
        let (result, _) = run(&source, &dirs).await;
        assert!(
            matches!(result, Err(HelperError::Download(_))),
            "{result:?}"
        );
        assert!(!part_path(&dirs, HelperKind::Cloudflared, "9.9.9").exists());
    }

    #[tokio::test]
    async fn a_length_above_the_pin_is_refused_before_writing() {
        let tmp = TempDir::new("biglen");
        let response = b"HTTP/1.0 200 OK\r\ncontent-length: 5000000\r\n\r\nabc".to_vec();
        let source = capped(serve_raw(response).await, 1 << 20);
        let dirs = tmp.dirs();
        let (result, progress) = run(&source, &dirs).await;
        assert!(
            matches!(result, Err(HelperError::Download(_))),
            "{result:?}"
        );
        assert!(!part_path(&dirs, HelperKind::Cloudflared, "9.9.9").exists());
        assert_eq!(progress.borrow().received, 0);
    }

    #[tokio::test]
    async fn a_redirect_to_http_is_refused() {
        let tmp = TempDir::new("redir-http");
        let response =
            b"HTTP/1.1 302 Found\r\nlocation: http://127.0.0.1:1/x\r\ncontent-length: 0\r\n\r\n"
                .to_vec();
        let source = capped(serve_raw(response).await, 1 << 20);
        let (result, _) = run(&source, &tmp.dirs()).await;
        assert!(
            matches!(result, Err(HelperError::Download(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn a_redirect_to_another_host_is_refused() {
        let tmp = TempDir::new("redir-host");
        let response = b"HTTP/1.1 302 Found\r\nlocation: https://evil.example.invalid/x\r\ncontent-length: 0\r\n\r\n".to_vec();
        let source = capped(serve_raw(response).await, 1 << 20);
        let (result, _) = run(&source, &tmp.dirs()).await;
        assert!(
            matches!(result, Err(HelperError::Download(_))),
            "{result:?}"
        );
    }
}
