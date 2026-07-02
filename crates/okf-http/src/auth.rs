use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{UserAuthorization, UserStore};

pub(crate) const PAIRING_TTL: Duration = Duration::from_secs(5 * 60);
pub(crate) const SESSION_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_ATTEMPTS_PER_WINDOW: u8 = 5;
const ATTEMPT_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub(crate) struct LocalAuthState {
    inner: Arc<Mutex<AuthInner>>,
    session_ttl: Duration,
}

impl fmt::Debug for LocalAuthState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalAuthState")
            .field("state", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug)]
struct AuthInner {
    pairing: Option<PairingState>,
    sessions: BTreeMap<String, SessionState>,
}

struct PairingState {
    code: String,
    expires_at: Instant,
    attempts_remaining: u8,
    blocked_until: Option<Instant>,
}

impl fmt::Debug for PairingState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PairingState")
            .field("code", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("attempts_remaining", &self.attempts_remaining)
            .field("blocked_until", &self.blocked_until)
            .finish()
    }
}

#[derive(Debug)]
struct SessionState {
    csrf_token: String,
    expires_at: Instant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SessionGrant {
    pub(crate) session_id: String,
    pub(crate) csrf_token: String,
    pub(crate) expires_in_seconds: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PairingError {
    Unavailable,
    Invalid,
    RateLimited,
    Randomness,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SessionStatus {
    pub(crate) authenticated: bool,
    pub(crate) expires_in_seconds: Option<u64>,
    pub(crate) csrf_token: Option<String>,
}

#[derive(Clone)]
pub(crate) struct PersistentAuthState {
    sessions: Arc<Mutex<BTreeMap<String, PersistentSession>>>,
    session_ttl: Duration,
}

impl fmt::Debug for PersistentAuthState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PersistentAuthState")
            .field("sessions", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Debug)]
struct PersistentSession {
    username: String,
    auth_epoch: i64,
    csrf_token: String,
    expires_at: Instant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistentSessionStatus {
    pub(crate) username: String,
    pub(crate) capabilities: Vec<String>,
    pub(crate) csrf_token: String,
    pub(crate) expires_in_seconds: u64,
}

impl PersistentAuthState {
    pub(crate) fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(BTreeMap::new())),
            session_ttl: SESSION_TTL,
        }
    }

    pub(crate) fn login(
        &self,
        authorization: UserAuthorization,
    ) -> Result<SessionGrant, PairingError> {
        let session_id = random_hex(32).map_err(|_| PairingError::Randomness)?;
        let csrf_token = random_hex(32).map_err(|_| PairingError::Randomness)?;
        self.sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                session_id.clone(),
                PersistentSession {
                    username: authorization.name,
                    auth_epoch: authorization.auth_epoch,
                    csrf_token: csrf_token.clone(),
                    expires_at: Instant::now() + self.session_ttl,
                },
            );
        Ok(SessionGrant {
            session_id,
            csrf_token,
            expires_in_seconds: self.session_ttl.as_secs(),
        })
    }

    pub(crate) fn status(
        &self,
        session_id: &str,
        users: &UserStore,
    ) -> Option<PersistentSessionStatus> {
        let now = Instant::now();
        let session = {
            let mut sessions = self
                .sessions
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            sessions.retain(|_, session| session.expires_at > now);
            sessions.get(session_id).cloned()
        }?;
        let authorization = users
            .current_authorization(&session.username)
            .ok()
            .flatten();
        let Some(authorization) = authorization else {
            self.logout(session_id);
            return None;
        };
        if authorization.auth_epoch != session.auth_epoch {
            self.logout(session_id);
            return None;
        }
        Some(PersistentSessionStatus {
            username: authorization.name,
            capabilities: authorization.capabilities,
            csrf_token: session.csrf_token,
            expires_in_seconds: session.expires_at.saturating_duration_since(now).as_secs(),
        })
    }

    pub(crate) fn logout(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(session_id)
            .is_some()
    }

    pub(crate) fn revoke_user(&self, username: &str) {
        self.sessions
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|_, session| session.username != username);
    }
}

impl LocalAuthState {
    pub(crate) fn new(pairing_code: Option<String>) -> Self {
        Self::with_ttls(pairing_code, PAIRING_TTL, SESSION_TTL)
    }

    fn with_ttls(
        pairing_code: Option<String>,
        pairing_ttl: Duration,
        session_ttl: Duration,
    ) -> Self {
        let now = Instant::now();
        Self {
            inner: Arc::new(Mutex::new(AuthInner {
                pairing: pairing_code.map(|code| PairingState {
                    code,
                    expires_at: now + pairing_ttl,
                    attempts_remaining: MAX_ATTEMPTS_PER_WINDOW,
                    blocked_until: None,
                }),
                sessions: BTreeMap::new(),
            })),
            session_ttl,
        }
    }

    pub(crate) fn pairing_available(&self) -> bool {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        prune_expired(&mut inner, now);
        inner.pairing.is_some()
    }

    pub(crate) fn pair(&self, provided_code: &str) -> Result<SessionGrant, PairingError> {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        prune_expired(&mut inner, now);
        let Some(pairing) = inner.pairing.as_mut() else {
            return Err(PairingError::Unavailable);
        };

        if pairing.blocked_until.is_some_and(|until| now < until) {
            return Err(PairingError::RateLimited);
        }
        if pairing.blocked_until.is_some() {
            pairing.blocked_until = None;
            pairing.attempts_remaining = MAX_ATTEMPTS_PER_WINDOW;
        }

        if !valid_pairing_code_format(provided_code)
            || !constant_time_eq(provided_code.as_bytes(), pairing.code.as_bytes())
        {
            pairing.attempts_remaining = pairing.attempts_remaining.saturating_sub(1);
            if pairing.attempts_remaining == 0 {
                pairing.blocked_until = Some(now + ATTEMPT_COOLDOWN);
                return Err(PairingError::RateLimited);
            }
            return Err(PairingError::Invalid);
        }

        let session_id = random_hex(32).map_err(|_| PairingError::Randomness)?;
        let csrf_token = random_hex(32).map_err(|_| PairingError::Randomness)?;
        inner.pairing = None;
        inner.sessions.insert(
            session_id.clone(),
            SessionState {
                csrf_token: csrf_token.clone(),
                expires_at: now + self.session_ttl,
            },
        );
        Ok(SessionGrant {
            session_id,
            csrf_token,
            expires_in_seconds: self.session_ttl.as_secs(),
        })
    }

    pub(crate) fn authorize_session(&self, session_id: &str, csrf_token: &str) -> bool {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        prune_expired(&mut inner, now);
        inner.sessions.get(session_id).is_some_and(|session| {
            constant_time_eq(session.csrf_token.as_bytes(), csrf_token.as_bytes())
        })
    }

    pub(crate) fn session_status(&self, session_id: Option<&str>) -> SessionStatus {
        let now = Instant::now();
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        prune_expired(&mut inner, now);
        let session = session_id.and_then(|id| inner.sessions.get(id));
        SessionStatus {
            authenticated: session.is_some(),
            expires_in_seconds: session
                .map(|session| session.expires_at.saturating_duration_since(now).as_secs()),
            csrf_token: session.map(|session| session.csrf_token.clone()),
        }
    }

    pub(crate) fn logout(&self, session_id: &str) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .sessions
            .remove(session_id)
            .is_some()
    }
}

pub(crate) fn generate_pairing_code() -> Result<String, String> {
    const CODE_SPACE: u64 = 1_000_000_000_000;
    const ACCEPTED_RANGE: u64 = u64::MAX - (u64::MAX % CODE_SPACE);
    let value = loop {
        let mut bytes = [0u8; 8];
        getrandom::getrandom(&mut bytes)
            .map_err(|error| format!("failed to generate pairing code: {error}"))?;
        let candidate = u64::from_le_bytes(bytes);
        if candidate < ACCEPTED_RANGE {
            break candidate % CODE_SPACE;
        }
    };
    let digits = format!("{value:012}");
    Ok(format!(
        "{}-{}-{}",
        &digits[0..4],
        &digits[4..8],
        &digits[8..12]
    ))
}

fn random_hex(length: usize) -> Result<String, getrandom::Error> {
    let mut bytes = vec![0u8; length];
    getrandom::getrandom(&mut bytes)?;
    let mut value = String::with_capacity(length * 2);
    for byte in bytes {
        let _ = write!(value, "{byte:02x}");
    }
    Ok(value)
}

fn valid_pairing_code_format(value: &str) -> bool {
    value.len() == 14
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 4 | 9) {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
}

fn prune_expired(inner: &mut AuthInner, now: Instant) {
    if inner
        .pairing
        .as_ref()
        .is_some_and(|pairing| pairing.expires_at <= now)
    {
        inner.pairing = None;
    }
    inner.sessions.retain(|_, session| session.expires_at > now);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_is_one_time_and_creates_an_expiring_session() {
        let state = LocalAuthState::new(Some("1234-5678-9012".to_string()));
        let grant = state.pair("1234-5678-9012").expect("pair");
        assert_eq!(grant.session_id.len(), 64);
        assert_eq!(grant.csrf_token.len(), 64);
        assert!(state.authorize_session(&grant.session_id, &grant.csrf_token));
        assert_eq!(state.pair("1234-5678-9012"), Err(PairingError::Unavailable));
    }

    #[test]
    fn failed_attempts_are_temporarily_rate_limited() {
        let state = LocalAuthState::new(Some("1234-5678-9012".to_string()));
        for _ in 0..4 {
            assert_eq!(state.pair("0000-0000-0000"), Err(PairingError::Invalid));
        }
        assert_eq!(state.pair("0000-0000-0000"), Err(PairingError::RateLimited));
        assert_eq!(state.pair("1234-5678-9012"), Err(PairingError::RateLimited));
    }

    #[test]
    fn expired_pairing_and_sessions_are_removed() {
        let pairing = LocalAuthState::with_ttls(
            Some("1234-5678-9012".to_string()),
            Duration::ZERO,
            Duration::from_secs(60),
        );
        assert!(!pairing.pairing_available());
        assert_eq!(
            pairing.pair("1234-5678-9012"),
            Err(PairingError::Unavailable)
        );

        let session = LocalAuthState::with_ttls(
            Some("1234-5678-9012".to_string()),
            Duration::from_secs(60),
            Duration::ZERO,
        );
        let grant = session.pair("1234-5678-9012").expect("pair");
        assert!(!session.authorize_session(&grant.session_id, &grant.csrf_token));
    }

    #[test]
    fn generated_codes_are_human_grouped_and_random() {
        let first = generate_pairing_code().expect("first code");
        let second = generate_pairing_code().expect("second code");
        assert_ne!(first, second);
        assert_eq!(first.len(), 14);
        assert!(valid_pairing_code_format(&first));
    }
}
