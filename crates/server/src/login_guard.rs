//! Bounded authentication throttling and trusted-proxy client address resolution.
//!
//! The limiter is keyed by the normalized account name (never the client-chosen
//! `DeviceId`) and by client IP. Both tables have a hard capacity, are pruned
//! opportunistically, and are only touched after the caller has validated the
//! username, so unauthenticated traffic cannot grow them without bound.

use std::{
    collections::{HashMap, VecDeque},
    convert::Infallible,
    hash::Hash,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{HeaderMap, request::Parts},
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tjxy_common::Username;

use crate::AppState;

const ACCOUNT_WINDOW: Duration = Duration::from_secs(15 * 60);
const ACCOUNT_MAX_FAILURES: usize = 10;
const IP_WINDOW: Duration = Duration::from_secs(60);
const IP_MAX_FAILURES: usize = 30;
const TABLE_CAPACITY: usize = 10_000;
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
const PASSKEY_CHALLENGE_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_PENDING_PASSKEY_CHALLENGES: usize = 2_048;
const MAX_FORWARDED_ENTRIES: usize = 64;

/// Normalized account identity used as a throttling key.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct AccountKey(Vec<u8>);

impl AccountKey {
    pub(crate) fn from_username(username: &Username) -> Self {
        Self(username.key().to_vec())
    }

    /// Derives a key from an already-stored display name, if it is still valid.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Username::parse(name)
            .ok()
            .map(|username| Self::from_username(&username))
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LimitScope {
    Account,
    Ip,
    Capacity,
}

impl LimitScope {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Ip => "ip",
            Self::Capacity => "capacity",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limited {
    pub(crate) retry_after: Duration,
    pub(crate) scope: LimitScope,
}

impl Limited {
    /// Whole seconds for a `Retry-After` header, never below one.
    pub(crate) fn retry_after_seconds(&self) -> u64 {
        self.retry_after
            .as_secs()
            .saturating_add(u64::from(self.retry_after.subsec_nanos() > 0))
            .max(1)
    }
}

/// A reserved authentication attempt. It counts as a failure until released.
#[derive(Debug)]
#[must_use]
pub(crate) struct Attempt {
    account: Option<AccountKey>,
    ip: IpAddr,
    at: Instant,
}

struct Bucket<Key> {
    window: Duration,
    max: usize,
    capacity: usize,
    entries: HashMap<Key, VecDeque<Instant>>,
}

impl<Key: Clone + Eq + Hash> Bucket<Key> {
    fn new(window: Duration, max: usize, capacity: usize) -> Self {
        Self {
            window,
            max,
            capacity,
            entries: HashMap::new(),
        }
    }

    fn prune(window: Duration, queue: &mut VecDeque<Instant>, now: Instant) {
        while queue
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= window)
        {
            queue.pop_front();
        }
    }

    /// Read-only lookup: an absent key is never inserted by a check.
    fn limited(&mut self, key: &Key, now: Instant) -> Option<Duration> {
        let window = self.window;
        let queue = self.entries.get_mut(key)?;
        Self::prune(window, queue, now);
        if queue.is_empty() {
            self.entries.remove(key);
            return None;
        }
        if queue.len() < self.max {
            return None;
        }
        let oldest = *queue.front()?;
        Some((oldest + window).saturating_duration_since(now))
    }

    fn push(&mut self, key: Key, at: Instant) {
        if !self.entries.contains_key(&key) && self.entries.len() >= self.capacity {
            self.sweep(at);
            if self.entries.len() >= self.capacity {
                let oldest = self
                    .entries
                    .iter()
                    .min_by_key(|(_, queue)| queue.back().copied())
                    .map(|(key, _)| key.clone());
                if let Some(oldest) = oldest {
                    self.entries.remove(&oldest);
                }
            }
        }
        self.entries.entry(key).or_default().push_back(at);
    }

    fn remove_one(&mut self, key: &Key, at: Instant) {
        if let Some(queue) = self.entries.get_mut(key) {
            if let Some(position) = queue.iter().rposition(|value| *value == at) {
                queue.remove(position);
            }
            if queue.is_empty() {
                self.entries.remove(key);
            }
        }
    }

    fn sweep(&mut self, now: Instant) {
        let window = self.window;
        self.entries.retain(|_, queue| {
            Self::prune(window, queue, now);
            !queue.is_empty()
        });
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

struct GuardState {
    accounts: Bucket<AccountKey>,
    ips: Bucket<IpAddr>,
    passkey_challenges: VecDeque<Instant>,
    last_sweep: Instant,
}

/// Shared authentication throttle for password, passkey, and `QuickConnect` flows.
pub(crate) struct LoginGuard {
    state: Mutex<GuardState>,
    fake_credential_secret: [u8; 32],
}

impl LoginGuard {
    pub(crate) fn new() -> Self {
        Self::with_capacity(TABLE_CAPACITY)
    }

    fn with_capacity(capacity: usize) -> Self {
        let mut secret = [0_u8; 32];
        getrandom::fill(&mut secret).expect("OS randomness unavailable");
        Self {
            state: Mutex::new(GuardState {
                accounts: Bucket::new(ACCOUNT_WINDOW, ACCOUNT_MAX_FAILURES, capacity),
                ips: Bucket::new(IP_WINDOW, IP_MAX_FAILURES, capacity),
                passkey_challenges: VecDeque::new(),
                last_sweep: Instant::now(),
            }),
            fake_credential_secret: secret,
        }
    }

    fn lock(&self) -> MutexGuard<'_, GuardState> {
        // The state is plain counters, so continuing after a poisoned lock is safe.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Checks both limits and, when allowed, reserves one failure slot.
    ///
    /// The reservation closes the race between concurrent attempts: it stays
    /// counted unless the caller calls [`Self::release`].
    pub(crate) fn begin(
        &self,
        account: Option<&AccountKey>,
        ip: IpAddr,
    ) -> Result<Attempt, Limited> {
        self.begin_at(account, ip, Instant::now())
    }

    fn begin_at(
        &self,
        account: Option<&AccountKey>,
        ip: IpAddr,
        now: Instant,
    ) -> Result<Attempt, Limited> {
        let mut state = self.lock();
        state.sweep_if_due(now);
        if let Some(retry_after) = state.ips.limited(&ip, now) {
            return Err(Limited {
                retry_after,
                scope: LimitScope::Ip,
            });
        }
        if let Some(account) = account
            && let Some(retry_after) = state.accounts.limited(account, now)
        {
            return Err(Limited {
                retry_after,
                scope: LimitScope::Account,
            });
        }
        state.ips.push(ip, now);
        if let Some(account) = account {
            state.accounts.push(account.clone(), now);
        }
        Ok(Attempt {
            account: account.cloned(),
            ip,
            at: now,
        })
    }

    /// Checks the account limit without reserving anything.
    pub(crate) fn check_account(&self, account: &AccountKey) -> Result<(), Limited> {
        self.check_account_at(account, Instant::now())
    }

    fn check_account_at(&self, account: &AccountKey, now: Instant) -> Result<(), Limited> {
        match self.lock().accounts.limited(account, now) {
            Some(retry_after) => Err(Limited {
                retry_after,
                scope: LimitScope::Account,
            }),
            None => Ok(()),
        }
    }

    /// Records one failure against an account outside a reserved attempt.
    pub(crate) fn record_account_failure(&self, account: &AccountKey) {
        self.lock().accounts.push(account.clone(), Instant::now());
    }

    /// Gives back an attempt's reservation (the outcome was not a credential failure).
    pub(crate) fn release(&self, attempt: &Attempt) {
        let mut state = self.lock();
        state.ips.remove_one(&attempt.ip, attempt.at);
        if let Some(account) = &attempt.account {
            state.accounts.remove_one(account, attempt.at);
        }
    }

    /// Forgets prior failures after a successful password login.
    pub(crate) fn clear_account(&self, account: &AccountKey) {
        self.lock().accounts.entries.remove(account);
    }

    /// Reserves capacity for one pending unauthenticated passkey challenge.
    pub(crate) fn try_reserve_passkey_challenge(&self) -> bool {
        self.try_reserve_passkey_challenge_at(Instant::now())
    }

    fn try_reserve_passkey_challenge_at(&self, now: Instant) -> bool {
        let mut state = self.lock();
        while state
            .passkey_challenges
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= PASSKEY_CHALLENGE_TTL)
        {
            state.passkey_challenges.pop_front();
        }
        if state.passkey_challenges.len() >= MAX_PENDING_PASSKEY_CHALLENGES {
            return false;
        }
        state.passkey_challenges.push_back(now);
        true
    }

    /// Stable, unguessable credential id for the decoy challenge of an unknown user.
    pub(crate) fn fake_credential_id(&self, identity: &[u8]) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(self.fake_credential_secret);
        digest.update(identity);
        digest.finalize().into()
    }

    #[cfg(test)]
    pub(crate) fn table_sizes(&self) -> (usize, usize) {
        let state = self.lock();
        (state.accounts.len(), state.ips.len())
    }
}

impl GuardState {
    fn sweep_if_due(&mut self, now: Instant) {
        if now.saturating_duration_since(self.last_sweep) >= SWEEP_INTERVAL {
            self.accounts.sweep(now);
            self.ips.sweep(now);
            self.last_sweep = now;
        }
    }
}

impl Default for LoginGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Error)]
pub enum TrustedProxiesError {
    #[error("trusted proxy entry {0:?} is not an IP address or CIDR range")]
    Invalid(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Cidr {
    address: IpAddr,
    prefix: u8,
}

impl Cidr {
    fn contains(self, candidate: IpAddr) -> bool {
        match (self.address.to_canonical(), candidate.to_canonical()) {
            (IpAddr::V4(network), IpAddr::V4(candidate)) => {
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix))
                    .unwrap_or(0);
                u32::from(network) & mask == u32::from(candidate) & mask
            }
            (IpAddr::V6(network), IpAddr::V6(candidate)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix))
                    .unwrap_or(0);
                u128::from(network) & mask == u128::from(candidate) & mask
            }
            _ => false,
        }
    }
}

/// Reverse proxies whose `X-Forwarded-For` header may be believed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrustedProxies(Vec<Cidr>);

impl TrustedProxies {
    /// Parses a comma-separated list of IP addresses and CIDR ranges.
    ///
    /// # Errors
    ///
    /// Returns [`TrustedProxiesError::Invalid`] for the first malformed entry.
    pub fn parse(value: &str) -> Result<Self, TrustedProxiesError> {
        let mut entries = Vec::new();
        for item in value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            let invalid = || TrustedProxiesError::Invalid(item.to_owned());
            let (address, prefix) = match item.split_once('/') {
                Some((address, prefix)) => (address.trim(), Some(prefix.trim())),
                None => (item, None),
            };
            let address = address.parse::<IpAddr>().map_err(|_| invalid())?;
            let maximum = if address.is_ipv4() { 32 } else { 128 };
            let prefix = match prefix {
                Some(prefix) => prefix
                    .parse::<u8>()
                    .ok()
                    .filter(|prefix| *prefix <= maximum)
                    .ok_or_else(invalid)?,
                None => maximum,
            };
            entries.push(Cidr { address, prefix });
        }
        Ok(Self(entries))
    }

    fn contains(&self, address: IpAddr) -> bool {
        self.0.iter().any(|cidr| cidr.contains(address))
    }

    /// Resolves the client address. `X-Forwarded-For` is only read when the
    /// direct peer is a trusted proxy; the rightmost entry that is not itself a
    /// trusted proxy wins. `X-Real-IP` is never consulted.
    pub(crate) fn client_ip(&self, peer: IpAddr, headers: &HeaderMap) -> IpAddr {
        let peer = peer.to_canonical();
        if !self.contains(peer) {
            return peer;
        }
        let mut hop = peer;
        let mut seen = 0_usize;
        for value in headers.get_all("x-forwarded-for").iter().rev() {
            let Ok(value) = value.to_str() else {
                return hop;
            };
            for entry in value.rsplit(',') {
                seen += 1;
                if seen > MAX_FORWARDED_ENTRIES {
                    return hop;
                }
                let Ok(address) = entry.trim().parse::<IpAddr>() else {
                    return hop;
                };
                let address = address.to_canonical();
                if !self.contains(address) {
                    return address;
                }
                hop = address;
            }
        }
        hop
    }
}

/// Client address after trusted-proxy resolution.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClientAddr(pub(crate) IpAddr);

impl FromRequestParts<AppState> for ClientAddr {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED), |info| info.0.ip());
        Ok(Self(state.trusted_proxies.client_ip(peer, &parts.headers)))
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap()
    }

    fn account(name: &str) -> AccountKey {
        AccountKey::from_name(name).unwrap()
    }

    fn headers(values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append("x-forwarded-for", HeaderValue::from_str(value).unwrap());
        }
        headers
    }

    #[test]
    fn case_and_unicode_variants_share_one_account_bucket() {
        assert_eq!(account("Alice"), account("aLiCe"));
        assert_eq!(account("ＡＬＩＣＥ"), account("alice"));
    }

    #[test]
    fn account_locks_after_ten_failures_and_reports_remaining_time() {
        let guard = LoginGuard::new();
        let now = Instant::now();
        let alice = account("alice");
        for offset in 0..10 {
            // Distinct IPs so only the account bucket can trip.
            let source = IpAddr::V4(Ipv4Addr::new(10, 0, 0, offset + 1));
            guard
                .begin_at(
                    Some(&alice),
                    source,
                    now + Duration::from_secs(u64::from(offset)),
                )
                .map(drop)
                .unwrap();
        }
        let limited = guard
            .begin_at(
                Some(&account("ALICE")),
                ip("10.0.1.1"),
                now + Duration::from_secs(60),
            )
            .unwrap_err();
        assert_eq!(limited.scope, LimitScope::Account);
        // The oldest failure leaves the window 15 minutes after it was recorded.
        assert_eq!(limited.retry_after, Duration::from_secs(15 * 60 - 60));
        assert_eq!(limited.retry_after_seconds(), 15 * 60 - 60);
        // After the window elapses the account is usable again.
        guard
            .begin_at(
                Some(&alice),
                ip("10.0.1.2"),
                now + Duration::from_secs(15 * 60 + 1),
            )
            .map(drop)
            .unwrap();
    }

    #[test]
    fn ip_limit_is_thirty_per_minute_and_independent_of_the_account() {
        let guard = LoginGuard::new();
        let now = Instant::now();
        let source = ip("203.0.113.9");
        for index in 0..30 {
            guard
                .begin_at(Some(&account(&format!("user{index}"))), source, now)
                .map(drop)
                .unwrap();
        }
        let limited = guard
            .begin_at(Some(&account("another")), source, now)
            .unwrap_err();
        assert_eq!(limited.scope, LimitScope::Ip);
        assert!(
            guard
                .begin_at(Some(&account("another")), source, now + IP_WINDOW)
                .is_ok()
        );
    }

    #[test]
    fn checking_a_locked_or_unknown_key_does_not_grow_the_tables() {
        let guard = LoginGuard::new();
        let now = Instant::now();
        let target = account("target");
        for offset in 0..10 {
            let source = IpAddr::V4(Ipv4Addr::new(10, 1, 0, offset + 1));
            guard
                .begin_at(Some(&target), source, now)
                .map(drop)
                .unwrap();
        }
        let before = guard.table_sizes();
        for host in 0..50 {
            let source = IpAddr::V4(Ipv4Addr::new(10, 2, 0, host + 1));
            assert!(guard.begin_at(Some(&target), source, now).is_err());
        }
        assert_eq!(guard.table_sizes(), before);
        assert!(guard.check_account_at(&account("never-seen"), now).is_ok());
        assert_eq!(guard.table_sizes(), before);
    }

    #[test]
    fn released_attempts_do_not_count_and_success_clears_the_account() {
        let guard = LoginGuard::new();
        let now = Instant::now();
        let alice = account("alice");
        let source = ip("192.0.2.1");
        for _ in 0..9 {
            let attempt = guard.begin_at(Some(&alice), source, now).unwrap();
            guard.release(&attempt);
        }
        assert_eq!(guard.table_sizes(), (0, 0));
        for _ in 0..9 {
            guard.begin_at(Some(&alice), source, now).map(drop).unwrap();
        }
        guard.clear_account(&alice);
        for _ in 0..9 {
            guard
                .begin_at(Some(&alice), ip("192.0.2.2"), now)
                .map(drop)
                .unwrap();
        }
    }

    #[test]
    fn tables_are_capacity_bounded_and_evict_the_oldest_entry() {
        let guard = LoginGuard::with_capacity(8);
        let now = Instant::now();
        for index in 0_u8..20 {
            let source = IpAddr::V4(Ipv4Addr::new(10, 3, 0, index));
            guard
                .begin_at(
                    Some(&account(&format!("user-{index}"))),
                    source,
                    now + Duration::from_millis(u64::from(index)),
                )
                .map(drop)
                .unwrap();
            let (accounts, ips) = guard.table_sizes();
            assert!(accounts <= 8 && ips <= 8);
        }
        let state = guard.lock();
        assert!(state.accounts.entries.contains_key(&account("user-19")));
        assert!(!state.accounts.entries.contains_key(&account("user-0")));
    }

    #[test]
    fn expired_entries_are_swept_before_live_ones_are_evicted() {
        let guard = LoginGuard::with_capacity(4);
        let now = Instant::now();
        for index in 0_u8..4 {
            guard
                .begin_at(
                    Some(&account(&format!("old-{index}"))),
                    IpAddr::V4(Ipv4Addr::new(10, 4, 0, index)),
                    now + Duration::from_millis(u64::from(index)),
                )
                .map(drop)
                .unwrap();
        }
        // The IP window (60 s) has elapsed but the account window has not.
        let later = now + Duration::from_secs(120);
        guard
            .begin_at(Some(&account("fresh")), ip("10.4.1.1"), later)
            .map(drop)
            .unwrap();
        let state = guard.lock();
        assert_eq!(state.ips.len(), 1, "expired IP entries must be swept");
        assert!(state.accounts.entries.contains_key(&account("old-3")));
    }

    #[test]
    fn pending_passkey_challenges_are_capped_and_expire() {
        let guard = LoginGuard::new();
        let now = Instant::now();
        for _ in 0..MAX_PENDING_PASSKEY_CHALLENGES {
            assert!(guard.try_reserve_passkey_challenge_at(now));
        }
        assert!(!guard.try_reserve_passkey_challenge_at(now));
        assert!(guard.try_reserve_passkey_challenge_at(now + PASSKEY_CHALLENGE_TTL));
    }

    #[test]
    fn fake_credential_ids_are_stable_per_account_and_differ_between_accounts() {
        let guard = LoginGuard::new();
        assert_eq!(
            guard.fake_credential_id(account("ghost").as_bytes()),
            guard.fake_credential_id(account("GHOST").as_bytes())
        );
        assert_ne!(
            guard.fake_credential_id(account("ghost").as_bytes()),
            guard.fake_credential_id(account("other").as_bytes())
        );
    }

    #[test]
    fn trusted_proxy_configuration_rejects_malformed_entries() {
        for invalid in [
            "nope",
            "10.0.0.0/33",
            "::1/129",
            "10.0.0.0/x",
            "1.2.3.4,bad",
        ] {
            assert!(TrustedProxies::parse(invalid).is_err(), "{invalid}");
        }
        assert_eq!(
            TrustedProxies::parse("").unwrap(),
            TrustedProxies::default()
        );
        assert!(TrustedProxies::parse(" 127.0.0.1 , 10.0.0.0/8, fd00::/8 ,").is_ok());
    }

    #[test]
    fn forwarded_for_is_ignored_unless_the_peer_is_a_trusted_proxy() {
        let proxies = TrustedProxies::parse("127.0.0.1").unwrap();
        let spoofed = headers(&["198.51.100.7"]);
        assert_eq!(
            proxies.client_ip(ip("203.0.113.5"), &spoofed),
            ip("203.0.113.5")
        );
        assert_eq!(
            TrustedProxies::default().client_ip(ip("127.0.0.1"), &spoofed),
            ip("127.0.0.1")
        );
        assert_eq!(
            proxies.client_ip(ip("127.0.0.1"), &spoofed),
            ip("198.51.100.7")
        );
    }

    #[test]
    fn rightmost_untrusted_forwarded_address_is_the_client() {
        let proxies = TrustedProxies::parse("127.0.0.1, 10.0.0.0/8, fd00::/8").unwrap();
        // The attacker prepends a fake address; the proxy chain appends the real one.
        let forwarded = headers(&["1.1.1.1, 198.51.100.7, 10.1.2.3"]);
        assert_eq!(
            proxies.client_ip(ip("127.0.0.1"), &forwarded),
            ip("198.51.100.7")
        );
        let split = headers(&["1.1.1.1", "198.51.100.8, fd00::5"]);
        assert_eq!(
            proxies.client_ip(ip("::ffff:127.0.0.1"), &split),
            ip("198.51.100.8")
        );
        // Garbage in the trusted portion never promotes an attacker-controlled value.
        let garbage = headers(&["198.51.100.9, not-an-ip, 10.0.0.2"]);
        assert_eq!(proxies.client_ip(ip("127.0.0.1"), &garbage), ip("10.0.0.2"));
        // Without any header the proxy itself is the best available identity.
        assert_eq!(
            proxies.client_ip(ip("127.0.0.1"), &HeaderMap::new()),
            ip("127.0.0.1")
        );
    }

    #[test]
    fn x_real_ip_is_never_trusted() {
        let proxies = TrustedProxies::parse("127.0.0.1").unwrap();
        let mut real_ip = HeaderMap::new();
        real_ip.insert("x-real-ip", HeaderValue::from_static("198.51.100.7"));
        assert_eq!(
            proxies.client_ip(ip("127.0.0.1"), &real_ip),
            ip("127.0.0.1")
        );
    }
}
