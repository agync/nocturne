//! The token, the tickets and the checks every request passes.

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

/// Equal strings, in time that does not depend on where they differ.
fn same(a: &str, b: &str) -> bool {
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

fn refuse(status: StatusCode, code: &str) -> Response {
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

/// Whether the request may go on. `Host` and `Origin` first (a page on another site, a name
/// that points here by trickery), then the token.
fn admit(
    preview: &Preview,
    method: &Method,
    headers: &HeaderMap,
    path: &str,
    query: Option<&str>,
) -> Result<(), (StatusCode, &'static str)> {
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
        return Ok(());
    }
    if bearer(headers).is_some_and(|t| same(t, &preview.opts.token)) {
        return Ok(());
    }
    if method == Method::GET {
        if path == "/api/events"
            && let Some(ticket) = query_value(query, "ticket")
            && preview.redeem(ticket, TicketKind::Events)
        {
            return Ok(());
        }
        if (path.starts_with("/originals/") || path.starts_with("/audio/"))
            && let Some(ticket) = query_value(query, "t")
            && preview.redeem(ticket, TicketKind::Media)
        {
            return Ok(());
        }
    }
    Err((StatusCode::UNAUTHORIZED, "unauthorized"))
}

pub async fn gate(
    State(preview): State<Arc<Preview>>,
    request: Request<Body>,
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
        Ok(()) => next.run(request).await,
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
    fn a_query_value_is_found_by_its_key() {
        assert_eq!(query_value(Some("a=1&ticket=xyz"), "ticket"), Some("xyz"));
        assert_eq!(query_value(Some("a=1"), "ticket"), None);
        assert_eq!(query_value(None, "t"), None);
    }
}
