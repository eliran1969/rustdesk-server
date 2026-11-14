use super::{
    db::{
        sessions::{self, SessionKind},
        users::{self, User},
    },
    error::ApiError,
    state::ConsoleState,
};
use crate::common::get_arg;
use axum::{
    async_trait,
    extract::{FromRequest, RequestParts},
    http::{header, HeaderMap},
};
use hbb_common::{
    log,
    tokio::{
        self,
        sync::{Semaphore, SemaphorePermit},
    },
};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

const MAX_FAILURES: u32 = 10;
const WINDOW: Duration = Duration::from_secs(60);
/// Concurrent password checks (login / change-password) across the whole console. Bounds
/// the CPU bcrypt can take from hbbs, and makes "check limit -> verify -> record failure"
/// effectively serial so parallel requests cannot overshoot the failure limit by much.
const VERIFY_SLOTS: usize = 2;

/// A valid bcrypt (cost 12) hash of a throwaway password. Login verifies against it when
/// the username is unknown, so the response time does not reveal whether a user exists.
const DUMMY_HASH: &str = "$2b$12$AGgzE3vLUWD5AJ8/Kw3JKeDn6QLRfF8mix7fDLRL.FAWU9FI7KGrW";

struct Failures {
    first: Instant,
    count: u32,
}

struct LimiterInner {
    entries: HashMap<IpAddr, Failures>,
    last_prune: Instant,
}

/// Per-IP login failure counter: more than `MAX_FAILURES` failures within a fixed
/// `WINDOW` (starting at the first failure) blocks further attempts until it expires.
/// Only failures are counted; memory is bounded by pruning expired entries.
pub(crate) struct LoginLimiter {
    inner: Mutex<LimiterInner>,
    verify_slots: Semaphore,
    max_failures: u32,
    window: Duration,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginLimiter {
    pub(crate) fn new() -> Self {
        Self::with_limits(MAX_FAILURES, WINDOW)
    }

    fn with_limits(max_failures: u32, window: Duration) -> Self {
        Self {
            inner: Mutex::new(LimiterInner {
                entries: HashMap::new(),
                last_prune: Instant::now(),
            }),
            verify_slots: Semaphore::new(VERIFY_SLOTS),
            max_failures,
            window,
        }
    }

    fn lock(&self) -> MutexGuard<'_, LimiterInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn allowed(&self, inner: &LimiterInner, ip: &IpAddr, now: Instant) -> bool {
        match inner.entries.get(ip) {
            Some(f) if now.duration_since(f.first) < self.window => f.count < self.max_failures,
            _ => true,
        }
    }

    /// Waits for one of the `VERIFY_SLOTS` password-check slots. Callers check the limit
    /// again after acquiring it.
    pub(crate) async fn verify_slot(&self) -> Result<SemaphorePermit<'_>, ApiError> {
        self.verify_slots
            .acquire()
            .await
            .map_err(|e| ApiError::Internal(format!("verify semaphore closed: {e}")))
    }

    /// Whether `ip` may attempt a login now.
    pub(crate) fn check(&self, ip: IpAddr) -> bool {
        let inner = self.lock();
        self.allowed(&inner, &ip, Instant::now())
    }

    pub(crate) fn fail(&self, ip: IpAddr) {
        let now = Instant::now();
        let window = self.window;
        let mut inner = self.lock();
        if now.duration_since(inner.last_prune) >= window {
            inner
                .entries
                .retain(|_, f| now.duration_since(f.first) < window);
            inner.last_prune = now;
        }
        let entry = inner.entries.entry(ip).or_insert(Failures {
            first: now,
            count: 0,
        });
        if now.duration_since(entry.first) >= window {
            entry.first = now;
            entry.count = 0;
        }
        entry.count = entry.count.saturating_add(1);
    }

    /// Clears the counter of `ip`, but only while it is under the limit: a correct
    /// password must not lift an active block.
    pub(crate) fn success(&self, ip: IpAddr) {
        let mut inner = self.lock();
        if self.allowed(&inner, &ip, Instant::now()) {
            inner.entries.remove(&ip);
        }
    }
}

/// The client address: the TCP peer (`ConnectInfo`), or the first `X-Forwarded-For`
/// entry when `TRUST_PROXY=Y`. Falls back to 127.0.0.1 when neither is available.
pub(crate) fn client_ip(headers: &HeaderMap, connect: Option<SocketAddr>) -> IpAddr {
    let trust_proxy = get_arg("TRUST_PROXY").eq_ignore_ascii_case("Y");
    client_ip_from(headers, connect, trust_proxy)
}

fn client_ip_from(headers: &HeaderMap, connect: Option<SocketAddr>, trust_proxy: bool) -> IpAddr {
    if trust_proxy {
        let forwarded = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .and_then(|v| v.trim().parse::<IpAddr>().ok());
        if let Some(ip) = forwarded {
            return ip;
        }
    }
    connect
        .map(|a| a.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

/// The token from `Authorization: Bearer <token>`, if present and non-empty.
pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return None;
    }
    let token = token.trim();
    (!token.is_empty()).then_some(token)
}

/// bcrypt verify off the async runtime. With `hash == None` (unknown user) it still
/// spends the same time on a dummy hash and returns false.
pub(crate) async fn verify_password_blocking(
    password: String,
    hash: Option<String>,
) -> Result<bool, ApiError> {
    tokio::task::spawn_blocking(move || match hash {
        Some(hash) => users::verify_password(&password, &hash),
        None => {
            users::verify_password(&password, DUMMY_HASH);
            false
        }
    })
    .await
    .map_err(|e| ApiError::Internal(format!("password verify task failed: {e}")))
}

/// bcrypt hash off the async runtime.
pub(crate) async fn hash_password_blocking(password: String) -> Result<String, ApiError> {
    tokio::task::spawn_blocking(move || users::hash_password(&password))
        .await
        .map_err(|e| ApiError::Internal(format!("password hash task failed: {e}")))?
        .map_err(ApiError::from)
}

fn console_state<B>(req: &RequestParts<B>) -> Result<ConsoleState, ApiError> {
    req.extensions()
        .get::<ConsoleState>()
        .cloned()
        .ok_or_else(|| ApiError::Internal("console state missing".to_owned()))
}

/// An authenticated, enabled user. Rejects with 401 when the bearer token is missing or
/// unknown, or when the user was deleted or disabled after logging in.
pub(crate) struct AuthUser(
    pub User,
    // Not yet consulted: every current route accepts both web and client sessions.
    #[allow(dead_code)] pub SessionKind,
);

#[async_trait]
impl<B: Send> FromRequest<B> for AuthUser {
    type Rejection = ApiError;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let state = console_state(req)?;
        let token_hash =
            sessions::hash_token(bearer_token(req.headers()).ok_or(ApiError::Unauthorized)?);
        let (guid, kind) = sessions::find_session(&state.db, &token_hash)
            .await?
            .ok_or(ApiError::Unauthorized)?;
        let user = users::find_by_guid(&state.db, &guid)
            .await?
            .ok_or(ApiError::Unauthorized)?;
        if user.status != 1 {
            return Err(ApiError::Unauthorized);
        }
        if let Err(e) = sessions::touch_session(&state.db, &token_hash).await {
            log::warn!("console: failed to update session last_seen: {}", e);
        }
        Ok(AuthUser(user, kind))
    }
}

/// An authenticated admin; 403 for authenticated non-admins.
// Consumed by the admin API in later tasks.
#[allow(dead_code)]
pub(crate) struct AdminUser(pub User);

#[async_trait]
impl<B: Send> FromRequest<B> for AdminUser {
    type Rejection = ApiError;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let AuthUser(user, _) = AuthUser::from_request(req).await?;
        if !user.is_admin {
            return Err(ApiError::Forbidden);
        }
        Ok(AdminUser(user))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn limiter_blocks_after_max_failures_and_success_does_not_unblock() {
        let l = LoginLimiter::new();
        let a = ip("10.0.0.1");
        for _ in 0..MAX_FAILURES {
            assert!(l.check(a));
            l.fail(a);
        }
        assert!(!l.check(a));
        l.success(a);
        assert!(!l.check(a));
        assert!(l.check(ip("10.0.0.2")));
    }

    #[test]
    fn limiter_success_resets_under_limit_and_window_expires() {
        let l = LoginLimiter::with_limits(2, Duration::from_millis(50));
        let a = ip("10.0.0.1");
        l.fail(a);
        l.success(a);
        l.fail(a);
        assert!(l.check(a));
        l.fail(a);
        assert!(!l.check(a));
        std::thread::sleep(Duration::from_millis(60));
        assert!(l.check(a));
        // Expired entries are pruned on the next insert.
        l.fail(ip("10.0.0.3"));
        assert!(!l.lock().entries.contains_key(&a));
    }

    #[test]
    fn client_ip_honors_forwarded_for_only_when_trusted() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "203.0.113.7, 10.0.0.1".parse().unwrap());
        let peer: SocketAddr = "192.0.2.1:4000".parse().unwrap();
        assert_eq!(client_ip_from(&h, Some(peer), false), ip("192.0.2.1"));
        assert_eq!(client_ip_from(&h, Some(peer), true), ip("203.0.113.7"));
        assert_eq!(
            client_ip_from(&HeaderMap::new(), None, true),
            ip("127.0.0.1")
        );
    }

    #[test]
    fn bearer_token_parsing() {
        let mut h = HeaderMap::new();
        assert_eq!(bearer_token(&h), None);
        h.insert(header::AUTHORIZATION, "Bearer abc".parse().unwrap());
        assert_eq!(bearer_token(&h), Some("abc"));
        h.insert(header::AUTHORIZATION, "Basic abc".parse().unwrap());
        assert_eq!(bearer_token(&h), None);
        h.insert(header::AUTHORIZATION, "Bearer ".parse().unwrap());
        assert_eq!(bearer_token(&h), None);
    }

    #[test]
    fn dummy_hash_is_valid_cost_12() {
        assert!(DUMMY_HASH.starts_with("$2b$12$"));
        assert!(bcrypt::verify("console-dummy-password", DUMMY_HASH).unwrap());
    }
}
