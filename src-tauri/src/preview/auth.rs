//! The token, the tickets and the checks every request passes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::{Preview, TicketKind};

/// `bytes` random bytes as hex.
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("the system has random bytes");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// A fresh 256-bit token.
pub fn new_token() -> String {
    random_hex(32)
}

/// What a request's bearer token is checked against.
#[derive(Clone)]
pub enum Token {
    /// Made at launch and held in memory: the development preview.
    Fixed(String),
    /// Kept owner-only in a file and read at each check, so a new token takes effect without a
    /// restart: the server.
    File(Arc<TokenFile>),
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Fixed(_) => f.write_str("Token::Fixed(..)"),
            Token::File(file) => write!(f, "Token::File({:?})", file.path),
        }
    }
}

impl From<&str> for Token {
    fn from(token: &str) -> Self {
        Token::Fixed(token.to_string())
    }
}

impl From<String> for Token {
    fn from(token: String) -> Self {
        Token::Fixed(token)
    }
}

impl Token {
    /// A token kept in `path`, which [`ensure_token_file`] made.
    pub fn file(path: &Path) -> Token {
        Token::File(Arc::new(TokenFile {
            path: path.to_path_buf(),
        }))
    }

    /// The token now; `None` when the file is missing or malformed.
    pub(crate) fn current(&self) -> Option<String> {
        match self {
            Token::Fixed(token) => Some(token.clone()),
            Token::File(file) => file.current(),
        }
    }

    #[cfg(test)]
    fn matches(&self, presented: &str) -> bool {
        self.current().is_some_and(|t| same(presented, &t))
    }
}

/// The token file. It is read at every request that needs it: a few bytes, and no cache that a
/// change within the file system's timestamp granularity could leave stale.
pub struct TokenFile {
    path: PathBuf,
}

impl TokenFile {
    /// The token in the file now. A missing, unreadable or malformed file admits nobody.
    fn current(&self) -> Option<String> {
        read_token(&self.path)
    }
}

/// A well-formed token from `path`: 64 hex digits, surrounding space ignored.
fn read_token(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let token = text.trim();
    (token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())).then(|| token.to_string())
}

/// The token in `path`, made (owner-only) when there is none or it is malformed. `true` when it
/// was made now.
pub fn ensure_token_file(path: &Path) -> std::io::Result<(String, bool)> {
    if let Some(token) = read_token(path) {
        crate::fsutil::restrict_file(path);
        return Ok((token, false));
    }
    rotate_token_file(path).map(|token| (token, true))
}

/// Replace the token in `path` with a new one. A running server takes it at the next request.
pub fn rotate_token_file(path: &Path) -> std::io::Result<String> {
    let token = new_token();
    crate::fsutil::write_private(path, format!("{token}\n").as_bytes())?;
    Ok(token)
}

/// Equal strings, in time that does not depend on where they differ.
pub(crate) fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ")
}

fn query_value<'a>(query: Option<&'a str>, key: &str) -> Option<&'a str> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(k, v)| (k == key).then_some(v))
}

/// Paths that carry data and so need the token (or a ticket for the two that allow one).
fn protected(path: &str) -> bool {
    path.starts_with("/api/") || path.starts_with("/originals/") || path.starts_with("/audio/")
}

pub(crate) fn refuse(status: StatusCode, code: &str) -> Response {
    let mut response = (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(serde_json::json!({ "code": code, "detail": null })),
    )
        .into_response();
    if status == StatusCode::UNAUTHORIZED {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            "Bearer".parse().expect("a header"),
        );
    }
    response
}

/// The token a request was admitted under, read once by the gate. What the request makes (a
/// ticket, an event stream) is bound to it, never to a token read again later, so a request let
/// in just before a new token cannot leave with access under the new one.
#[derive(Clone)]
pub(crate) struct Admitted(pub(crate) String);

/// Whether the request may go on, and under which token when it carries data. `Host` and
/// `Origin` first (a page on another site, a name that points here by trickery), then the token.
fn admit(
    preview: &Preview,
    method: &Method,
    headers: &HeaderMap,
    path: &str,
    query: Option<&str>,
) -> Result<Option<Admitted>, (StatusCode, &'static str)> {
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    if !host.is_some_and(|h| preview.hosts.iter().any(|ok| ok == h)) {
        return Err((StatusCode::FORBIDDEN, "forbidden"));
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let ok = origin
            .to_str()
            .is_ok_and(|o| preview.origins.iter().any(|ok| ok == o));
        if !ok {
            return Err((StatusCode::FORBIDDEN, "forbidden"));
        }
    }
    if !protected(path) {
        return Ok(None);
    }
    let Some(current) = preview.opts.token.current() else {
        return Err((StatusCode::UNAUTHORIZED, "unauthorized"));
    };
    if bearer(headers).is_some_and(|t| same(t, &current)) {
        return Ok(Some(Admitted(current)));
    }
    if method == Method::GET {
        if path == "/api/events"
            && let Some(ticket) = query_value(query, "ticket")
            && preview.redeem(ticket, TicketKind::Events, &current)
        {
            return Ok(Some(Admitted(current)));
        }
        if (path.starts_with("/originals/") || path.starts_with("/audio/"))
            && let Some(ticket) = query_value(query, "t")
            && preview.redeem(ticket, TicketKind::Media, &current)
        {
            return Ok(Some(Admitted(current)));
        }
    }
    Err((StatusCode::UNAUTHORIZED, "unauthorized"))
}

pub async fn gate(
    State(preview): State<Arc<Preview>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let verdict = admit(
        &preview,
        request.method(),
        request.headers(),
        request.uri().path(),
        request.uri().query(),
    );
    match verdict {
        Ok(admitted) => {
            if let Some(admitted) = admitted {
                request.extensions_mut().insert(admitted);
            }
            next.run(request).await
        }
        Err((status, code)) => refuse(status, code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_and_different() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn comparison_needs_every_byte() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "ab"));
    }

    #[test]
    fn a_token_file_is_made_owner_only_and_kept_across_starts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server-token");
        let (made, fresh) = ensure_token_file(&path).unwrap();
        assert!(fresh);
        assert_eq!(made.len(), 64);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let (again, fresh) = ensure_token_file(&path).unwrap();
        assert!(!fresh);
        assert_eq!(again, made);
    }

    #[test]
    fn a_malformed_token_file_is_replaced_and_admits_nobody_until_then() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server-token");
        std::fs::write(&path, "short\n").unwrap();
        let token = Token::file(&path);
        assert!(!token.matches("short"));
        assert!(!token.matches(""));
        let (made, fresh) = ensure_token_file(&path).unwrap();
        assert!(fresh);
        assert!(token.matches(&made));
    }

    #[test]
    fn a_rotated_token_takes_effect_and_the_old_one_stops_working() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server-token");
        let (old, _) = ensure_token_file(&path).unwrap();
        let token = Token::file(&path);
        assert!(token.matches(&old));
        let new = rotate_token_file(&path).unwrap();
        assert_ne!(new, old);
        assert!(token.matches(&new));
        assert!(!token.matches(&old));
        std::fs::remove_file(&path).unwrap();
        assert!(!token.matches(&new));
    }

    #[test]
    fn the_token_is_never_in_debug_output() {
        let text = format!("{:?}", Token::from("secret-value"));
        assert!(!text.contains("secret-value"), "{text}");
    }

    #[test]
    fn a_query_value_is_found_by_its_key() {
        assert_eq!(query_value(Some("a=1&ticket=xyz"), "ticket"), Some("xyz"));
        assert_eq!(query_value(Some("a=1"), "ticket"), None);
        assert_eq!(query_value(None, "t"), None);
    }
}
