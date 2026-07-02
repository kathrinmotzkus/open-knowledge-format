use axum::http::StatusCode;
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct Envelope<T> {
    pub(crate) api_version: &'static str,
    pub(crate) data: T,
}

#[derive(Serialize)]
pub(crate) struct ErrorEnvelope {
    pub(crate) api_version: &'static str,
    pub(crate) error: ErrorBody,
}

#[derive(Serialize)]
pub(crate) struct ErrorBody {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

pub(crate) fn error_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::BAD_REQUEST => "bad_request",
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::CONFLICT => "conflict",
        StatusCode::PRECONDITION_REQUIRED => "precondition_required",
        StatusCode::TOO_MANY_REQUESTS => "rate_limited",
        StatusCode::BAD_GATEWAY => "provider_error",
        StatusCode::SERVICE_UNAVAILABLE => "service_unavailable",
        _ if status.is_server_error() => "internal_error",
        _ => "request_failed",
    }
}
