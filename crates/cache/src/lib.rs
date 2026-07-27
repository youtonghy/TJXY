//! Redis cache-aside key contracts. SQL revisions remain authoritative.

use std::{
    collections::HashMap,
    fmt,
    net::IpAddr,
    str::FromStr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use redis::{AsyncCommands, aio::ConnectionManagerConfig};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::watch;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheKeyBuilder {
    prefix: String,
}

impl CacheKeyBuilder {
    /// Creates a versioned cache key builder.
    ///
    /// # Errors
    ///
    /// Returns [`CacheKeyError`] when the prefix is empty or contains the key
    /// segment delimiter or Redis glob metacharacters.
    pub fn new(prefix: impl Into<String>) -> Result<Self, CacheKeyError> {
        Ok(Self {
            prefix: validate_segment(prefix.into())?,
        })
    }

    #[must_use]
    pub fn user_scoped(
        &self,
        catalog_generation: i64,
        user_id: &str,
        user_revision: i64,
        projection: CacheProjection,
        query_hash: &CacheQueryDigest,
    ) -> String {
        format!(
            "{}:v1:g:{catalog_generation}:u:{user_id}:r:{user_revision}:{projection}:{query_hash}",
            self.prefix
        )
    }

    #[must_use]
    pub fn catalog_item(&self, catalog_generation: i64, item_id: &str) -> String {
        format!("{}:v1:g:{catalog_generation}:item:{item_id}", self.prefix)
    }

    #[must_use]
    pub fn playback_info(
        &self,
        catalog_generation: i64,
        user_id: &str,
        user_revision: i64,
        item_id: &str,
        probe_digest: &PlaybackProbeDigest,
    ) -> String {
        format!(
            "{}:v1:g:{catalog_generation}:u:{user_id}:r:{user_revision}:playback:{item_id}:p:{}",
            self.prefix, probe_digest.0
        )
    }

    /// Builds the Redis key registry for one catalog generation.
    ///
    /// # Errors
    ///
    /// Returns [`CacheKeyError`] when the generation is negative.
    pub fn generation_registry(&self, generation: i64) -> Result<String, CacheKeyError> {
        if generation < 0 {
            return Err(CacheKeyError);
        }
        Ok(format!("{}:v1:g:{generation}:keys", self.prefix))
    }

    fn registry_for_key(&self, key: &str) -> Option<String> {
        let suffix = key.strip_prefix(&format!("{}:v1:g:", self.prefix))?;
        let generation = suffix.split_once(':')?.0.parse::<i64>().ok()?;
        self.generation_registry(generation).ok()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheQueryDigest(String);

impl CacheQueryDigest {
    #[must_use]
    pub fn from_bytes(value: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(value)))
    }
}

impl fmt::Display for CacheQueryDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheProjection {
    UserViews,
    Latest,
    Resume,
    NextUp,
    Items,
    Search,
}

impl fmt::Display for CacheProjection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::UserViews => "user-views",
            Self::Latest => "latest",
            Self::Resume => "resume",
            Self::NextUp => "next-up",
            Self::Items => "items",
            Self::Search => "search",
        };
        formatter.write_str(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackProbeDigest(String);

impl PlaybackProbeDigest {
    /// Creates a digest segment from persisted `MediaSource` probe revisions.
    ///
    /// # Errors
    ///
    /// Returns [`CacheKeyError`] when the digest is empty or contains the key
    /// segment delimiter.
    pub fn new(value: impl Into<String>) -> Result<Self, CacheKeyError> {
        Ok(Self(validate_segment(value.into())?))
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("cache key segment must be non-empty and cannot contain delimiters or glob syntax")]
pub struct CacheKeyError;

fn validate_segment(value: String) -> Result<String, CacheKeyError> {
    if value.is_empty()
        || value.contains(':')
        || value.contains('*')
        || value.contains('?')
        || value.contains('[')
        || value.contains('\\')
    {
        return Err(CacheKeyError);
    }
    Ok(value)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedisMode {
    Auto,
    Enabled,
    Disabled,
}

impl FromStr for RedisMode {
    type Err = CacheConfigurationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "auto" => Ok(Self::Auto),
            "enabled" => Ok(Self::Enabled),
            "disabled" => Ok(Self::Disabled),
            _ => Err(CacheConfigurationError::InvalidMode),
        }
    }
}

#[derive(Clone)]
pub struct RedisCacheConfig {
    mode: RedisMode,
    url: String,
    keys: CacheKeyBuilder,
    timeout: Duration,
    home_ttl: Duration,
    item_ttl: Duration,
    empty_expansion_ttl: Duration,
}

impl fmt::Debug for RedisCacheConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedisCacheConfig")
            .field("mode", &self.mode)
            .field("url", &"[REDACTED]")
            .field("keys", &self.keys)
            .field("timeout", &self.timeout)
            .field("home_ttl", &self.home_ttl)
            .field("item_ttl", &self.item_ttl)
            .field("empty_expansion_ttl", &self.empty_expansion_ttl)
            .finish()
    }
}

impl RedisCacheConfig {
    /// Creates a bounded Redis cache configuration.
    ///
    /// # Errors
    ///
    /// Returns [`CacheConfigurationError`] when the prefix or timeout is invalid.
    pub fn new(
        mode: RedisMode,
        url: impl Into<String>,
        key_prefix: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, CacheConfigurationError> {
        if timeout.is_zero() {
            return Err(CacheConfigurationError::InvalidTimeout);
        }
        Ok(Self {
            mode,
            url: url.into(),
            keys: CacheKeyBuilder::new(key_prefix).map_err(|_| CacheConfigurationError::Prefix)?,
            timeout,
            home_ttl: Duration::from_secs(300),
            item_ttl: Duration::from_secs(1_800),
            empty_expansion_ttl: Duration::from_secs(3),
        })
    }

    /// Overrides the three bounded cache TTL classes.
    ///
    /// # Errors
    ///
    /// Returns [`CacheConfigurationError::InvalidTtl`] when any TTL is zero.
    pub fn with_ttls(
        mut self,
        home_ttl: Duration,
        item_ttl: Duration,
        empty_expansion_ttl: Duration,
    ) -> Result<Self, CacheConfigurationError> {
        if home_ttl.is_zero() || item_ttl.is_zero() || empty_expansion_ttl.is_zero() {
            return Err(CacheConfigurationError::InvalidTtl);
        }
        self.home_ttl = home_ttl;
        self.item_ttl = item_ttl;
        self.empty_expansion_ttl = empty_expansion_ttl;
        Ok(self)
    }

    #[must_use]
    pub const fn keys(&self) -> &CacheKeyBuilder {
        &self.keys
    }

    #[must_use]
    pub const fn home_ttl(&self) -> Duration {
        self.home_ttl
    }

    #[must_use]
    pub const fn item_ttl(&self) -> Duration {
        self.item_ttl
    }

    #[must_use]
    pub const fn empty_expansion_ttl(&self) -> Duration {
        self.empty_expansion_ttl
    }
}

impl Default for RedisCacheConfig {
    fn default() -> Self {
        Self {
            mode: RedisMode::Auto,
            url: "redis://127.0.0.1:6379".to_owned(),
            keys: CacheKeyBuilder {
                prefix: "tjxy".to_owned(),
            },
            timeout: Duration::from_millis(200),
            home_ttl: Duration::from_secs(300),
            item_ttl: Duration::from_secs(1_800),
            empty_expansion_ttl: Duration::from_secs(3),
        }
    }
}

#[derive(Clone)]
pub enum CacheRuntime {
    Disabled,
    Redis(RedisCache),
}

impl fmt::Debug for CacheRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("CacheRuntime")
            .field(&if self.is_enabled() {
                "Redis"
            } else {
                "Disabled"
            })
            .finish()
    }
}

impl CacheRuntime {
    /// Connects according to the configured mode and verifies Redis with `PING`.
    ///
    /// `Auto` degrades to [`CacheRuntime::Disabled`] when a local Redis is unavailable.
    /// `Enabled` returns an error so startup/readiness cannot report a false healthy state.
    ///
    /// # Errors
    ///
    /// Returns [`CacheStartupError`] for unsafe auto endpoints, invalid URLs, or required
    /// connection failures.
    pub async fn connect(config: RedisCacheConfig) -> Result<Self, CacheStartupError> {
        if config.mode == RedisMode::Disabled {
            return Ok(Self::Disabled);
        }
        let client = redis::Client::open(config.url.as_str())
            .map_err(|_| CacheStartupError::InvalidClientConfiguration)?;
        if config.mode == RedisMode::Auto && !is_local(client.get_connection_info().addr()) {
            return Err(CacheStartupError::NonLocalAutoEndpoint);
        }
        let manager_config = ConnectionManagerConfig::new()
            .set_number_of_retries(0)
            .set_connection_timeout(Some(config.timeout))
            .set_response_timeout(Some(config.timeout));
        let connection =
            redis::aio::ConnectionManager::new_with_config(client, manager_config).await;
        let mut connection = match connection {
            Ok(connection) => connection,
            Err(_) if config.mode == RedisMode::Auto => return Ok(Self::Disabled),
            Err(error) => return Err(CacheStartupError::Connection(error)),
        };
        let ping = redis::cmd("PING")
            .query_async::<String>(&mut connection)
            .await;
        match ping {
            Ok(response) if response == "PONG" => Ok(Self::Redis(RedisCache {
                connection,
                circuit: Arc::new(Mutex::new(CircuitState::default())),
                keys: config.keys,
            })),
            Ok(_) | Err(_) if config.mode == RedisMode::Auto => Ok(Self::Disabled),
            Ok(_) => Err(CacheStartupError::InvalidPing),
            Err(error) => Err(CacheStartupError::Connection(error)),
        }
    }

    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        matches!(self, Self::Redis(_))
    }

    #[must_use]
    pub fn health(&self) -> CacheHealth {
        match self {
            Self::Disabled => CacheHealth::Disabled,
            Self::Redis(cache) if circuit_degraded(&cache.circuit) => CacheHealth::Degraded,
            Self::Redis(_) => CacheHealth::Healthy,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheHealth {
    Disabled,
    Healthy,
    Degraded,
}

#[derive(Clone)]
pub struct RedisCache {
    connection: redis::aio::ConnectionManager,
    circuit: Arc<Mutex<CircuitState>>,
    keys: CacheKeyBuilder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheInvalidationOutcome {
    Deleted { count: usize, remaining: usize },
    SkippedDisabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheInvalidationFailureKind {
    InvalidGeneration,
    Unavailable,
    RedisCommand,
}

impl fmt::Display for CacheInvalidationFailureKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidGeneration => "InvalidGeneration",
            Self::Unavailable => "RedisUnavailable",
            Self::RedisCommand => "RedisCommandFailed",
        })
    }
}

#[derive(Debug, Error)]
pub enum CacheInvalidationError {
    #[error("catalog generation must not be negative")]
    InvalidGeneration,
    #[error("Redis cache invalidation is temporarily unavailable")]
    Unavailable,
    #[error("Redis cache invalidation command failed")]
    Redis(#[source] redis::RedisError),
}

impl CacheInvalidationError {
    #[must_use]
    pub const fn kind(&self) -> CacheInvalidationFailureKind {
        match self {
            Self::InvalidGeneration => CacheInvalidationFailureKind::InvalidGeneration,
            Self::Unavailable => CacheInvalidationFailureKind::Unavailable,
            Self::Redis(_) => CacheInvalidationFailureKind::RedisCommand,
        }
    }
}

#[async_trait]
pub trait CacheInvalidator: Send + Sync {
    async fn invalidate_generation(
        &self,
        generation: i64,
    ) -> Result<CacheInvalidationOutcome, CacheInvalidationError>;
}

#[async_trait]
impl CacheInvalidator for CacheRuntime {
    async fn invalidate_generation(
        &self,
        generation: i64,
    ) -> Result<CacheInvalidationOutcome, CacheInvalidationError> {
        if generation < 0 {
            return Err(CacheInvalidationError::InvalidGeneration);
        }
        match self {
            Self::Disabled => Ok(CacheInvalidationOutcome::SkippedDisabled),
            Self::Redis(cache) => cache.invalidate_generation(generation).await,
        }
    }
}

#[async_trait]
pub trait CacheStore: Send + Sync {
    async fn get(&self, key: &str) -> Option<Vec<u8>>;
    async fn put(&self, key: &str, value: &[u8], ttl: Duration);
    async fn delete(&self, key: &str);
}

#[async_trait]
impl CacheStore for CacheRuntime {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        let Self::Redis(cache) = self else {
            return None;
        };
        cache.get(key).await
    }

    async fn put(&self, key: &str, value: &[u8], ttl: Duration) {
        if let Self::Redis(cache) = self {
            cache.put(key, value, ttl).await;
        }
    }

    async fn delete(&self, key: &str) {
        if let Self::Redis(cache) = self {
            cache.delete(key).await;
        }
    }
}

impl RedisCache {
    async fn invalidate_generation(
        &self,
        generation: i64,
    ) -> Result<CacheInvalidationOutcome, CacheInvalidationError> {
        if circuit_open(&self.circuit) {
            return Err(CacheInvalidationError::Unavailable);
        }
        let registry = self
            .keys
            .generation_registry(generation)
            .map_err(|_| CacheInvalidationError::InvalidGeneration)?;
        let script = redis::Script::new(
            r"
local keys = redis.call('SPOP', KEYS[1], ARGV[1])
local deleted = 0
if #keys > 0 then
    deleted = redis.call('DEL', unpack(keys))
end
local remaining = redis.call('SCARD', KEYS[1])
if remaining == 0 then
    redis.call('DEL', KEYS[1])
end
return {deleted, remaining}
",
        );
        let mut connection = self.connection.clone();
        match script
            .key(registry)
            .arg(100_usize)
            .invoke_async::<(usize, usize)>(&mut connection)
            .await
        {
            Ok((count, remaining)) => {
                mark_success(&self.circuit);
                Ok(CacheInvalidationOutcome::Deleted { count, remaining })
            }
            Err(error) => Err(self.invalidation_failure(error)),
        }
    }

    fn invalidation_failure(&self, error: redis::RedisError) -> CacheInvalidationError {
        mark_failure(&self.circuit);
        CacheInvalidationError::Redis(error)
    }

    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        if circuit_open(&self.circuit) {
            return None;
        }
        let mut connection = self.connection.clone();
        if let Ok(value) = connection.get(key).await {
            mark_success(&self.circuit);
            value
        } else {
            mark_failure(&self.circuit);
            None
        }
    }

    async fn put(&self, key: &str, value: &[u8], ttl: Duration) {
        if circuit_open(&self.circuit) || ttl.is_zero() {
            return;
        }
        let seconds = ttl.as_secs().max(1);
        let mut connection = self.connection.clone();
        let result = if let Some(registry) = self.keys.registry_for_key(key) {
            let registry_ttl = seconds.saturating_add(3_600);
            redis::Script::new(
                r"
redis.call('SETEX', KEYS[1], ARGV[1], ARGV[2])
redis.call('SADD', KEYS[2], KEYS[1])
local current_ttl = redis.call('TTL', KEYS[2])
local requested_ttl = tonumber(ARGV[3])
if current_ttl < requested_ttl then
    redis.call('EXPIRE', KEYS[2], requested_ttl)
end
return 1
",
            )
            .key(key)
            .key(registry)
            .arg(seconds)
            .arg(value)
            .arg(registry_ttl)
            .invoke_async::<usize>(&mut connection)
            .await
            .map(|_| ())
        } else {
            connection.set_ex(key, value, seconds).await
        };
        if result.is_ok() {
            mark_success(&self.circuit);
        } else {
            mark_failure(&self.circuit);
        }
    }

    async fn delete(&self, key: &str) {
        if circuit_open(&self.circuit) {
            return;
        }
        let mut connection = self.connection.clone();
        let result: redis::RedisResult<usize> = connection.del(key).await;
        if result.is_ok() {
            mark_success(&self.circuit);
        } else {
            mark_failure(&self.circuit);
        }
    }
}

#[derive(Default)]
struct CircuitState {
    consecutive_failures: u8,
    open_until: Option<Instant>,
}

fn circuit_open(circuit: &Mutex<CircuitState>) -> bool {
    let mut state = circuit
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match state.open_until {
        Some(until) if until > Instant::now() => true,
        Some(_) => {
            state.open_until = None;
            state.consecutive_failures = 0;
            false
        }
        None => false,
    }
}

fn circuit_degraded(circuit: &Mutex<CircuitState>) -> bool {
    let mut state = circuit
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state
        .open_until
        .is_some_and(|until| until <= Instant::now())
    {
        state.open_until = None;
        state.consecutive_failures = 0;
    }
    state.consecutive_failures > 0
}

fn mark_success(circuit: &Mutex<CircuitState>) {
    let mut state = circuit
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.consecutive_failures = 0;
    state.open_until = None;
}

fn mark_failure(circuit: &Mutex<CircuitState>) {
    let mut state = circuit
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
    if state.consecutive_failures >= 3 {
        state.open_until = Some(Instant::now() + Duration::from_secs(1));
    }
}

fn is_local(address: &redis::ConnectionAddr) -> bool {
    match address {
        redis::ConnectionAddr::Tcp(host, _) | redis::ConnectionAddr::TcpTls { host, .. } => {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        }
        redis::ConnectionAddr::Unix(_) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_cache_failure_is_observable_as_degraded_health() {
        let circuit = Mutex::new(CircuitState::default());

        mark_failure(&circuit);

        assert!(circuit_degraded(&circuit));
    }
}

#[derive(Clone)]
pub struct SingleFlight {
    inner: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
    capacity: usize,
    wait_timeout: Duration,
}

impl SingleFlight {
    /// Creates bounded keyed cache-fill coordination.
    ///
    /// # Errors
    ///
    /// Returns [`SingleFlightError`] when capacity or wait timeout is zero.
    pub fn new(capacity: usize, wait_timeout: Duration) -> Result<Self, SingleFlightError> {
        if capacity == 0 || wait_timeout.is_zero() {
            return Err(SingleFlightError);
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            capacity,
            wait_timeout,
        })
    }

    #[must_use]
    pub fn enter(&self, key: &str) -> CacheFillPermit {
        let mut active = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(sender) = active.get(key) {
            return CacheFillPermit::Waiter(CacheFillWaiter {
                receiver: sender.subscribe(),
                wait_timeout: self.wait_timeout,
            });
        }
        if active.len() >= self.capacity {
            return CacheFillPermit::Bypass;
        }
        let (sender, _) = watch::channel(false);
        active.insert(key.to_owned(), sender.clone());
        CacheFillPermit::Leader(CacheFillLeader {
            key: key.to_owned(),
            active: Arc::clone(&self.inner),
            sender: Some(sender),
        })
    }
}

impl Default for SingleFlight {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            capacity: 128,
            wait_timeout: Duration::from_millis(50),
        }
    }
}

pub enum CacheFillPermit {
    Leader(CacheFillLeader),
    Waiter(CacheFillWaiter),
    Bypass,
}

pub struct CacheFillLeader {
    key: String,
    active: Arc<Mutex<HashMap<String, watch::Sender<bool>>>>,
    sender: Option<watch::Sender<bool>>,
}

impl Drop for CacheFillLeader {
    fn drop(&mut self) {
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.key);
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(true);
        }
    }
}

pub struct CacheFillWaiter {
    receiver: watch::Receiver<bool>,
    wait_timeout: Duration,
}

impl CacheFillWaiter {
    /// Waits only for the configured cache-fill window.
    ///
    /// Returns false on timeout or leader cancellation so the caller can read SQL directly.
    pub async fn wait(mut self) -> bool {
        if *self.receiver.borrow() {
            return true;
        }
        tokio::time::timeout(self.wait_timeout, self.receiver.changed())
            .await
            .is_ok_and(|changed| changed.is_ok() && *self.receiver.borrow())
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("single-flight capacity and wait timeout must be positive")]
pub struct SingleFlightError;

#[derive(Debug, Error)]
pub enum CacheConfigurationError {
    #[error("Redis mode must be auto, enabled, or disabled")]
    InvalidMode,
    #[error("cache key prefix is invalid")]
    Prefix,
    #[error("cache timeout must be positive")]
    InvalidTimeout,
    #[error("cache TTL values must be positive")]
    InvalidTtl,
}

#[derive(Debug, Error)]
pub enum CacheStartupError {
    #[error("Redis client configuration is invalid")]
    InvalidClientConfiguration,
    #[error("Redis auto mode only permits loopback or Unix socket endpoints")]
    NonLocalAutoEndpoint,
    #[error("required Redis connection failed: {0}")]
    Connection(redis::RedisError),
    #[error("Redis PING returned an unexpected response")]
    InvalidPing,
}
