use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

use crate::routing::{AccessClass, RouteContract};
use crate::{api_error, AppState, InternalErrorDetail, API_VERSION};

pub(crate) const SESSION_TOKEN_HEADER: &str = "x-okf-session-token";
pub(crate) const CSRF_TOKEN_HEADER: &str = "x-okf-csrf-token";
pub(crate) const SESSION_COOKIE_NAME: &str = "okf_session";
pub(crate) const REQUEST_ID_HEADER: &str = "x-request-id";
pub(crate) const TRUSTED_PROXY_TOKEN_HEADER: &str = "x-okf-trusted-proxy-token";
static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

const PAIRED_CAPABILITIES: &[&str] = &[
    "content.initialize",
    "content.write",
    "derived.rebuild",
    "review.decide",
    "roots.configure",
    "roots.propose",
    "session.logout",
    "session.recover",
    "voyage.spend",
];

pub(crate) async fn enforce_route_authorization(
    contract: RouteContract,
    state: AppState,
    request: Request<Body>,
    next: Next,
) -> Response {
    if state.trusted_proxy.is_some() {
        if let Some(response) = validate_trusted_proxy_request(&state, request.headers(), false) {
            return response;
        }
    }
    if contract.access == AccessClass::AnonymousRead {
        if (state.remote_access || state.trusted_proxy.is_some())
            && !is_remote_public_bootstrap_path(request.uri().path())
        {
            if let Some(response) = authorize_remote_read(&state, request.headers()) {
                return response;
            }
        }
        return next.run(request).await;
    }
    if contract.access == AccessClass::AuthenticationBootstrap {
        if let Some(response) = authorize_strict_same_origin(&state, request.headers()) {
            return response;
        }
        return next.run(request).await;
    }
    if let Some(response) =
        authorize_with_capabilities(&state, request.headers(), contract.capabilities)
    {
        return response;
    }
    next.run(request).await
}

pub(crate) async fn add_security_headers(request: Request<Body>, next: Next) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let legacy_api = path.starts_with("/api/okf/");
    let api_request = path.starts_with("/api/");
    let request_id = format!(
        "okf-{:016x}",
        REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let started = Instant::now();
    let mut response = next.run(request).await;
    let status = response.status();
    let internal_error = response
        .extensions()
        .get::<InternalErrorDetail>()
        .map(|detail| detail.0.clone());
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self'; font-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'; object-src 'none'"
            .parse()
            .expect("valid CSP"),
    );
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    headers.insert(header::X_FRAME_OPTIONS, "DENY".parse().unwrap());
    headers.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    headers.insert(
        "permissions-policy",
        "camera=(), microphone=(), geolocation=(), payment=(), usb=()"
            .parse()
            .unwrap(),
    );
    headers.insert("x-okf-api-version", API_VERSION.parse().unwrap());
    headers.insert(REQUEST_ID_HEADER, request_id.parse().unwrap());
    if api_request {
        headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    }
    if legacy_api {
        headers.insert("deprecation", "true".parse().unwrap());
        headers.insert("sunset", "Wed, 31 Dec 2026 23:59:59 GMT".parse().unwrap());
        headers.insert(
            header::LINK,
            "</api/v1/>; rel=\"successor-version\"".parse().unwrap(),
        );
    }
    eprintln!(
        "{}",
        serde_json::json!({
            "event": "http_request",
            "request_id": request_id,
            "method": method,
            "path": path,
            "status": status.as_u16(),
            "duration_ms": started.elapsed().as_millis(),
            "error": internal_error,
        })
    );
    response
}

pub(crate) fn authorize_sensitive_request(
    state: &AppState,
    headers: &HeaderMap,
) -> Option<Response> {
    authorize_with_capabilities(state, headers, &[])
}

fn authorize_with_capabilities(
    state: &AppState,
    headers: &HeaderMap,
    required_capabilities: &[&str],
) -> Option<Response> {
    if (state.remote_access || state.trusted_proxy.is_some())
        && required_capabilities.iter().any(|capability| {
            matches!(
                *capability,
                "roots.propose" | "roots.configure" | "content.initialize"
            )
        })
    {
        return Some(api_error(
            StatusCode::FORBIDDEN,
            "document-root management is disabled for remote deployments",
        ));
    }
    let provided = headers
        .get(SESSION_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !provided.is_empty()
        && !state.session_token.is_empty()
        && constant_time_eq(provided.as_bytes(), state.session_token.as_bytes())
    {
        if required_capabilities.iter().any(|capability| {
            matches!(
                *capability,
                "roots.propose" | "roots.configure" | "content.initialize"
            )
        }) {
            return Some(api_error(
                StatusCode::FORBIDDEN,
                "document-root management requires a paired or persistent authenticated session",
            ));
        }
        if required_capabilities.iter().any(|capability| {
            matches!(
                *capability,
                "users.manage" | "security.manage" | "password.change"
            )
        }) {
            return Some(api_error(
                StatusCode::FORBIDDEN,
                "transitional automation authority cannot administer persistent security",
            ));
        }
        return validate_legacy_request_origin(headers);
    }

    let Some(session_id) = session_cookie(headers) else {
        return Some(api_error(
            StatusCode::UNAUTHORIZED,
            "missing or invalid local editor authority",
        ));
    };
    let csrf_token = headers
        .get(CSRF_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if csrf_token.is_empty() {
        return Some(api_error(
            StatusCode::FORBIDDEN,
            format!("paired session requires {CSRF_TOKEN_HEADER}"),
        ));
    }
    if let Some(response) = authorize_strict_same_origin(state, headers) {
        return Some(response);
    }
    if state.local_auth.authorize_session(session_id, csrf_token) {
        if required_capabilities
            .iter()
            .all(|required| PAIRED_CAPABILITIES.contains(required))
        {
            return None;
        }
        return Some(api_error(
            StatusCode::FORBIDDEN,
            "paired local editor session lacks the required capability",
        ));
    }
    if let Some(users) = state.users.as_ref() {
        if let Some(status) = state.persistent_auth.status(session_id, users) {
            if !constant_time_eq(status.csrf_token.as_bytes(), csrf_token.as_bytes()) {
                return Some(api_error(
                    StatusCode::UNAUTHORIZED,
                    "invalid or expired session",
                ));
            }
            if required_capabilities.iter().all(|required| {
                status
                    .capabilities
                    .iter()
                    .any(|capability| capability == required)
            }) {
                return None;
            }
            return Some(api_error(
                StatusCode::FORBIDDEN,
                "session lacks the required capability",
            ));
        }
    }
    Some(api_error(
        StatusCode::UNAUTHORIZED,
        "invalid or expired session",
    ))
}

pub(crate) fn authorize_pairing_request(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    authorize_strict_same_origin(state, headers)
}

pub(crate) fn session_cookie(headers: &HeaderMap) -> Option<&str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(name, value)| (name == SESSION_COOKIE_NAME).then_some(value))
        .filter(|value| !value.is_empty())
}

fn authorize_strict_same_origin(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    if let Some(response) = validate_fetch_metadata(headers) {
        return Some(response);
    }

    if state.trusted_proxy.is_some() {
        return validate_trusted_proxy_request(state, headers, true);
    }
    if has_any_forwarding_header(headers) {
        return Some(api_error(
            StatusCode::FORBIDDEN,
            "forwarded headers require explicit trusted-proxy mode",
        ));
    }

    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !host.eq_ignore_ascii_case(&state.expected_host) {
        return Some(api_error(
            StatusCode::FORBIDDEN,
            "request host does not match the OKF HTTP listener",
        ));
    }

    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let scheme = if state.mode == crate::ServerMode::AuthenticatedTls {
        "https"
    } else {
        "http"
    };
    let expected_origin = format!("{scheme}://{}", state.expected_host);
    if !origin.eq_ignore_ascii_case(&expected_origin) {
        return Some(api_error(
            StatusCode::FORBIDDEN,
            "request origin does not match the OKF HTTP origin",
        ));
    }

    None
}

fn authorize_remote_read(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    let Some(session_id) = session_cookie(headers) else {
        return Some(api_error(
            StatusCode::UNAUTHORIZED,
            "remote document reading requires a persistent account",
        ));
    };
    let Some(users) = state.users.as_ref() else {
        return Some(api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "persistent authentication state is unavailable",
        ));
    };
    let authorized = state
        .persistent_auth
        .status(session_id, users)
        .is_some_and(|status| {
            status
                .capabilities
                .iter()
                .any(|capability| capability == "content.read")
        });
    (!authorized).then(|| api_error(StatusCode::UNAUTHORIZED, "invalid or expired session"))
}

fn is_remote_public_bootstrap_path(path: &str) -> bool {
    path == "/"
        || path == "/health"
        || path == "/docs-browser"
        || path == "/docs-browser/"
        || path.starts_with("/docs-browser/")
        || path == "/api/v1/health"
        || path == "/api/v1/access/session"
}

fn validate_trusted_proxy_request(
    state: &AppState,
    headers: &HeaderMap,
    require_origin: bool,
) -> Option<Response> {
    let proxy = state.trusted_proxy.as_ref()?;
    let host = match single_header(headers, header::HOST) {
        Ok(value) => value,
        Err(response) => return Some(*response),
    };
    if !host.eq_ignore_ascii_case(&state.expected_host) {
        return Some(api_error(
            StatusCode::FORBIDDEN,
            "trusted proxy must address the configured OKF backend host",
        ));
    }
    let token = match single_header(headers, TRUSTED_PROXY_TOKEN_HEADER) {
        Ok(value) => value,
        Err(response) => return Some(*response),
    };
    if !constant_time_eq(token.as_bytes(), proxy.token().as_bytes()) {
        return Some(api_error(StatusCode::FORBIDDEN, "untrusted reverse proxy"));
    }
    let forwarded_proto = match single_header(headers, "x-forwarded-proto") {
        Ok(value) => value,
        Err(response) => return Some(*response),
    };
    if forwarded_proto != "https" {
        return Some(api_error(
            StatusCode::FORBIDDEN,
            "trusted proxy requests must originate from HTTPS",
        ));
    }
    let forwarded_host = match single_header(headers, "x-forwarded-host") {
        Ok(value) => value,
        Err(response) => return Some(*response),
    };
    if !forwarded_host.eq_ignore_ascii_case(proxy.public_authority()) {
        return Some(api_error(
            StatusCode::FORBIDDEN,
            "forwarded host does not match the configured public origin",
        ));
    }
    if headers.contains_key("forwarded") {
        return Some(api_error(
            StatusCode::FORBIDDEN,
            "the ambiguous Forwarded header is not accepted; use validated X-Forwarded fields",
        ));
    }
    if let Some(forwarded_for) = headers.get("x-forwarded-for") {
        let valid = forwarded_for
            .to_str()
            .ok()
            .filter(|value| !value.contains(','))
            .and_then(|value| value.trim().parse::<std::net::IpAddr>().ok())
            .is_some();
        if !valid || headers.get_all("x-forwarded-for").iter().count() != 1 {
            return Some(api_error(
                StatusCode::FORBIDDEN,
                "ambiguous forwarded client identity",
            ));
        }
    }
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    if require_origin && origin.is_none() {
        return Some(api_error(
            StatusCode::FORBIDDEN,
            "trusted proxy sensitive requests require an Origin header",
        ));
    }
    if origin.is_some_and(|origin| !origin.eq_ignore_ascii_case(proxy.public_origin())) {
        return Some(api_error(
            StatusCode::FORBIDDEN,
            "request origin does not match the configured public origin",
        ));
    }
    None
}

fn single_header(
    headers: &HeaderMap,
    name: impl axum::http::header::AsHeaderName,
) -> Result<&str, Box<Response>> {
    let values = headers.get_all(name);
    let mut values = values.iter();
    let Some(value) = values.next() else {
        return Err(Box::new(api_error(
            StatusCode::FORBIDDEN,
            "required proxy header is missing",
        )));
    };
    if values.next().is_some() {
        return Err(Box::new(api_error(
            StatusCode::FORBIDDEN,
            "ambiguous repeated proxy header",
        )));
    }
    let value = value.to_str().map_err(|_| {
        Box::new(api_error(
            StatusCode::FORBIDDEN,
            "proxy header is not valid text",
        ))
    })?;
    if value.contains(',') || value.trim() != value || value.is_empty() {
        return Err(Box::new(api_error(
            StatusCode::FORBIDDEN,
            "ambiguous proxy header value",
        )));
    }
    Ok(value)
}

fn has_any_forwarding_header(headers: &HeaderMap) -> bool {
    [
        "forwarded",
        "x-forwarded-for",
        "x-forwarded-host",
        "x-forwarded-proto",
        TRUSTED_PROXY_TOKEN_HEADER,
    ]
    .iter()
    .any(|name| headers.contains_key(*name))
}

fn validate_legacy_request_origin(headers: &HeaderMap) -> Option<Response> {
    if let Some(response) = validate_fetch_metadata(headers) {
        return Some(response);
    }

    if let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        let host = headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        let matches_host = origin.eq_ignore_ascii_case(&format!("http://{host}"))
            || origin.eq_ignore_ascii_case(&format!("https://{host}"));
        if host.is_empty() || !matches_host {
            return Some(api_error(
                StatusCode::FORBIDDEN,
                "request origin does not match the OKF HTTP origin",
            ));
        }
    }

    None
}

fn validate_fetch_metadata(headers: &HeaderMap) -> Option<Response> {
    if let Some(site) = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
    {
        if !matches!(site, "same-origin" | "none") {
            return Some(api_error(
                StatusCode::FORBIDDEN,
                "cross-site sensitive request rejected",
            ));
        }
    }
    None
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let length = left.len().max(right.len());
    for index in 0..length {
        let left_byte = left.get(index).copied().unwrap_or(0);
        let right_byte = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(left_byte ^ right_byte);
    }
    difference == 0
}
