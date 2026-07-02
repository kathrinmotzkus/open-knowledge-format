use std::ffi::OsString;
use std::fmt::Write as _;

pub(crate) fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

pub(crate) fn parse_session_token(value: OsString) -> Result<String, &'static str> {
    let value = value
        .into_string()
        .map_err(|_| "session token must be valid UTF-8")?;
    if value.len() < 32
        || value.len() > 512
        || value
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || !c.is_ascii_graphic())
    {
        return Err("session token must contain 32 to 512 non-whitespace ASCII characters");
    }
    Ok(value)
}

pub(crate) fn generate_session_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| format!("failed to generate session token: {error}"))?;
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(token, "{byte:02x}");
    }
    Ok(token)
}
