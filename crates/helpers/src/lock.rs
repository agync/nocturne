//! The pinned helpers: `helpers.lock` at the repository root, embedded at build time.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::HelperKind;

/// Platform keys, the same as `lemonade.lock`.
pub const PLATFORMS: [&str; 4] = [
    "linux-x86_64",
    "linux-aarch64",
    "macos-aarch64",
    "windows-x86_64",
];

const EMBEDDED: &str = include_str!("../../../helpers.lock");

/// The platform key of this computer, if Nocturne ships helpers for it.
pub fn platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Some("linux-x86_64"),
        ("linux", "aarch64") => Some("linux-aarch64"),
        ("macos", "aarch64") => Some("macos-aarch64"),
        ("windows", "x86_64") => Some("windows-x86_64"),
        _ => None,
    }
}

/// True for the all-zero checksum that stands for "not pinned yet".
pub fn is_unpinned(sha256: &str) -> bool {
    !sha256.is_empty() && sha256.bytes().all(|b| b == b'0')
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelpersLock {
    pub helpers: Vec<LockedHelper>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedHelper {
    pub kind: String,
    pub version: String,
    pub licence: String,
    pub pinned_at: String,
    pub base_url: String,
    /// The date upstream published this version; release freshness counts from here.
    #[serde(default)]
    pub released_at: Option<String>,
    /// For a helper Nocturne builds itself: its source folder, the Go toolchain and the
    /// go-licenses version, all pinned here.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub go_version: Option<String>,
    #[serde(default)]
    pub go_licenses_version: Option<String>,
    #[serde(default)]
    pub placeholder: bool,
    /// Hosts a download may be redirected to, besides the one in `base_url`.
    #[serde(default)]
    pub redirect_hosts: Vec<String>,
    pub assets: BTreeMap<String, LockedAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedAsset {
    pub file: String,
    /// Replaces the helper's `base_url` for this platform.
    #[serde(default)]
    pub base_url: Option<String>,
    /// Lowercase hex of the download.
    pub sha256: String,
    /// Exact size in bytes of the download.
    #[serde(default)]
    pub size: u64,
    /// The binary inside an archive, by its exact name.
    #[serde(default)]
    pub member: Option<String>,
    /// Lowercase hex of the binary inside an archive.
    #[serde(default)]
    pub member_sha256: Option<String>,
}

impl HelpersLock {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// The lock this build uses: the embedded one. A build with the `fake` feature may name another
    /// in `NOCTURNE_FAKE_HELPERS_LOCK`, so tests and the network check can pin their stand-ins.
    pub fn current() -> HelpersLock {
        #[cfg(feature = "fake")]
        if let Some(path) = std::env::var_os("NOCTURNE_FAKE_HELPERS_LOCK") {
            tracing::warn!("{}: using a stand-in helpers lock", crate::FAKE_MARKER);
            if let Ok(text) = std::fs::read_to_string(path)
                && let Ok(lock) = Self::parse(&text)
            {
                return lock;
            }
        }
        Self::parse(EMBEDDED).expect("helpers.lock is valid")
    }

    pub fn embedded_text() -> &'static str {
        EMBEDDED
    }

    pub fn helper(&self, kind: HelperKind) -> Option<&LockedHelper> {
        self.helpers.iter().find(|h| h.kind == kind.name())
    }

    /// Everything wrong with the lock, empty when it is well formed.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        for kind in [HelperKind::Cloudflared, HelperKind::Tailnet] {
            if self.helper(kind).is_none() {
                out.push(format!("{}: not in the lock", kind.name()));
            }
        }
        for h in &self.helpers {
            let who = &h.kind;
            if h.version.is_empty() || h.licence.is_empty() || h.pinned_at.is_empty() {
                out.push(format!(
                    "{who}: version, licence and pinned_at are required"
                ));
            }
            if !h.base_url.starts_with("https://") {
                out.push(format!("{who}: base_url must be https"));
            }
            let github = std::iter::once(h.base_url.as_str())
                .chain(h.assets.values().filter_map(|a| a.base_url.as_deref()))
                .any(|u| host_of(u) == Some("github.com"));
            if github && !h.redirect_hosts.iter().any(|r| r == GITHUB_DOWNLOADS) {
                out.push(format!(
                    "{who}: a github.com download redirects, so redirect_hosts needs {GITHUB_DOWNLOADS}"
                ));
            }
            for platform in PLATFORMS {
                let Some(a) = h.assets.get(platform) else {
                    out.push(format!("{who}: no asset for {platform}"));
                    continue;
                };
                let at = format!("{who} {platform}");
                if !is_hex64(&a.sha256) {
                    out.push(format!("{at}: sha256 is not 64 lowercase hex digits"));
                }
                if a.base_url
                    .as_deref()
                    .is_some_and(|u| !u.starts_with("https://"))
                {
                    out.push(format!("{at}: base_url must be https"));
                }
                if a.file.is_empty() || a.file.contains(['/', '\\']) || a.file.contains("..") {
                    out.push(format!("{at}: file must be a plain name"));
                }
                match (&a.member, &a.member_sha256) {
                    (Some(m), Some(s)) => {
                        if !safe_member(m) {
                            out.push(format!("{at}: member {m:?} is not a plain relative path"));
                        }
                        if !is_hex64(s) {
                            out.push(format!(
                                "{at}: member_sha256 is not 64 lowercase hex digits"
                            ));
                        }
                    }
                    (None, None) => {}
                    _ => out.push(format!("{at}: member and member_sha256 go together")),
                }
            }
        }
        out
    }
}

impl LockedHelper {
    pub fn asset_url(&self, asset: &LockedAsset) -> String {
        let base = asset.base_url.as_deref().unwrap_or(&self.base_url);
        format!("{}/{}", base.trim_end_matches('/'), asset.file)
    }
}

/// Where github.com sends a release download.
const GITHUB_DOWNLOADS: &str = "release-assets.githubusercontent.com";

fn host_of(url: &str) -> Option<&str> {
    url.strip_prefix("https://")?.split(['/', ':']).next()
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A member name that stays inside the archive: relative, no `..`, no backslash.
pub fn safe_member(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.split('/').any(|part| part == ".." || part.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_covers_every_platform() {
        let lock = HelpersLock::current();
        assert_eq!(lock.problems(), Vec::<String>::new());
        for kind in [HelperKind::Cloudflared, HelperKind::Tailnet] {
            let h = lock.helper(kind).unwrap();
            for platform in PLATFORMS {
                assert!(h.assets.contains_key(platform), "{kind:?} {platform}");
            }
        }
    }

    #[test]
    fn a_missing_platform_and_a_bad_hash_are_reported() {
        let mut lock = HelpersLock::parse(EMBEDDED).unwrap();
        lock.helpers[0].assets.remove("macos-aarch64");
        lock.helpers[1]
            .assets
            .get_mut("linux-x86_64")
            .unwrap()
            .sha256 = "ABC".into();
        let problems = lock.problems().join("\n");
        assert!(
            problems.contains("no asset for macos-aarch64"),
            "{problems}"
        );
        assert!(problems.contains("sha256 is not 64"), "{problems}");
    }

    #[test]
    fn the_tailnet_helper_is_one_bare_binary_built_here() {
        let lock = HelpersLock::current();
        let h = lock.helper(HelperKind::Tailnet).unwrap();
        assert_eq!(h.source.as_deref(), Some("helpers/tailnet"));
        assert_eq!(h.go_version.as_deref(), Some("go1.27.3"));
        for platform in PLATFORMS {
            let a = &h.assets[platform];
            assert!(
                a.member.is_none() && a.member_sha256.is_none(),
                "{platform}"
            );
            let exe = if platform.starts_with("windows") {
                ".exe"
            } else {
                ""
            };
            assert_eq!(a.file, format!("nocturne-tailnet-{platform}{exe}"));
        }
        let old = EMBEDDED.replacen("size = 0", "size = 0\nextras = []", 1);
        assert!(
            HelpersLock::parse(&old).is_err(),
            "an unknown field is refused"
        );
    }

    #[test]
    fn a_github_download_needs_its_redirect_host() {
        let mut lock = HelpersLock::parse(EMBEDDED).unwrap();
        assert_eq!(lock.problems(), Vec::<String>::new());
        for h in &lock.helpers {
            assert!(
                h.redirect_hosts.iter().any(|r| r == GITHUB_DOWNLOADS),
                "{}",
                h.kind
            );
        }
        let cloudflared = &mut lock.helpers[0];
        assert!(cloudflared.base_url.starts_with("https://github.com/"));
        cloudflared.redirect_hosts.clear();
        let problems = lock.problems().join("\n");
        assert!(problems.contains(GITHUB_DOWNLOADS), "{problems}");
        let top = format!("unknown = 1\n{EMBEDDED}");
        assert!(
            HelpersLock::parse(&top).is_err(),
            "a top-level unknown field is refused"
        );
    }

    #[test]
    fn members_cannot_leave_the_archive() {
        assert!(safe_member("pkg/cloudflared"));
        assert!(!safe_member("../cloudflared"));
        assert!(!safe_member("/cloudflared"));
        assert!(!safe_member("a//b"));
        assert!(!safe_member("a\\b"));
    }

    #[test]
    fn unpinned_means_all_zeros() {
        assert!(is_unpinned(&"0".repeat(64)));
        assert!(!is_unpinned("0123"));
        assert!(!is_unpinned(""));
    }
}
