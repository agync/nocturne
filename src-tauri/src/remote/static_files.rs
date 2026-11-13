//! The phone app's own files, served unsealed with strict headers: `/`, `/p` and `/r` give the
//! app's page; the rest are its built files by exact name. Nothing else is served.

use std::path::PathBuf;

use nocturne_contract::RemoteConnection;

/// Where the phone app's built files come from: the embedded app assets in a release, the
/// built folder in development, a temporary folder in tests.
pub trait PhoneFiles: Send + Sync + 'static {
    fn get(&self, name: &str) -> Option<Vec<u8>>;
}

/// Files from a folder (`ui/dist/phone` in development).
pub struct DirFiles(pub PathBuf);

impl PhoneFiles for DirFiles {
    fn get(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(name)).ok()
    }
}

/// The file a request path names, or None: no dot segments, no hidden files, a short plain
/// alphabet only.
pub fn file_for(path: &str) -> Option<String> {
    match path {
        "/" | "/p" | "/r" | "/index.html" => return Some("index.html".to_string()),
        _ => {}
    }
    let name = path.strip_prefix('/')?;
    let ok = !name.is_empty()
        && name.len() <= 128
        && name.split('/').all(|seg| {
            !seg.is_empty()
                && !seg.starts_with('.')
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
        && mime_for(name).is_some();
    ok.then(|| name.to_string())
}

pub fn mime_for(name: &str) -> Option<&'static str> {
    let ext = name.rsplit_once('.')?.1;
    Some(match ext {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "webmanifest" => "application/manifest+json",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" => "text/plain; charset=utf-8",
        _ => return None,
    })
}

/// Who the phone page may talk to: the current address only, by connection.
pub fn connect_pattern(connection: RemoteConnection) -> &'static str {
    match connection {
        RemoteConnection::Simple => "https://*.trycloudflare.com wss://*.trycloudflare.com",
        RemoteConnection::MostPrivate => "https://*.ts.net wss://*.ts.net",
    }
}

/// The headers every phone file carries.
pub fn phone_headers(connection: RemoteConnection) -> Vec<(&'static str, String)> {
    vec![
        (
            "content-security-policy",
            format!(
                "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; \
                 media-src blob:; font-src 'self'; connect-src 'self' {}; worker-src 'self'; \
                 manifest-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
                connect_pattern(connection)
            ),
        ),
        ("referrer-policy", "no-referrer".into()),
        ("x-content-type-options", "nosniff".into()),
        ("cross-origin-opener-policy", "same-origin".into()),
        (
            "permissions-policy",
            "camera=(self), microphone=(self), geolocation=()".into(),
        ),
        ("cache-control", "no-cache".into()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_names_are_files() {
        assert_eq!(file_for("/p").as_deref(), Some("index.html"));
        assert_eq!(
            file_for("/assets/app-1a2b.js").as_deref(),
            Some("assets/app-1a2b.js")
        );
        for bad in [
            "/../secret.js",
            "/assets/../../x.js",
            "/.env",
            "/a//b.js",
            "/s",
            "/x.exe",
            "/a%2e%2e/b.js",
            "",
        ] {
            assert_eq!(file_for(bad), None, "{bad}");
        }
    }
}
