use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use thiserror::Error;
use tjxy_common::UserId;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, TryAcquireError};

const WINDOW: Duration = Duration::from_secs(60);
const MAX_REQUESTS_PER_MINUTE: u32 = 1_000;
const MAX_USER_CONCURRENT_SSE: usize = 100;
const MAX_GLOBAL_CONCURRENT_SSE: usize = 1_000;
const MAX_DAILY_QUOTA: u32 = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiAdmissionConfig {
    requests_per_minute: u32,
    max_user_concurrent_sse: usize,
    max_global_concurrent_sse: usize,
    daily_quota: u32,
}

impl AiAdmissionConfig {
    pub fn new(
        requests_per_minute: u32,
        max_user_concurrent_sse: usize,
        max_global_concurrent_sse: usize,
        daily_quota: u32,
    ) -> Result<Self, AiAdmissionConfigError> {
        if requests_per_minute == 0 || requests_per_minute > MAX_REQUESTS_PER_MINUTE {
            return Err(AiAdmissionConfigError::RequestsPerMinute);
        }
        if max_user_concurrent_sse == 0
            || max_user_concurrent_sse > MAX_USER_CONCURRENT_SSE
            || max_user_concurrent_sse > max_global_concurrent_sse
        {
            return Err(AiAdmissionConfigError::UserConcurrency);
        }
        if max_global_concurrent_sse == 0 || max_global_concurrent_sse > MAX_GLOBAL_CONCURRENT_SSE {
            return Err(AiAdmissionConfigError::GlobalConcurrency);
        }
        if daily_quota == 0 || daily_quota > MAX_DAILY_QUOTA {
            return Err(AiAdmissionConfigError::DailyQuota);
        }
        Ok(Self {
            requests_per_minute,
            max_user_concurrent_sse,
            max_global_concurrent_sse,
            daily_quota,
        })
    }

    #[must_use]
    pub const fn requests_per_minute(self) -> u32 {
        self.requests_per_minute
    }

    #[must_use]
    pub const fn max_user_concurrent_sse(self) -> usize {
        self.max_user_concurrent_sse
    }

    #[must_use]
    pub const fn max_global_concurrent_sse(self) -> usize {
        self.max_global_concurrent_sse
    }

    #[must_use]
    pub const fn daily_quota(self) -> u32 {
        self.daily_quota
    }
}

impl Default for AiAdmissionConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 10,
            max_user_concurrent_sse: 2,
            max_global_concurrent_sse: 8,
            daily_quota: 100,
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AiAdmissionConfigError {
    #[error("requests per minute must be from 1 through 1000")]
    RequestsPerMinute,
    #[error("per-user SSE concurrency must be from 1 through 100 and not exceed the global cap")]
    UserConcurrency,
    #[error("global SSE concurrency must be from 1 through 1000")]
    GlobalConcurrency,
    #[error("daily quota must be from 1 through 100000")]
    DailyQuota,
}

pub(crate) struct AiAdmissionController {
    config: AiAdmissionConfig,
    global_streams: Arc<Semaphore>,
    users: Mutex<HashMap<UserId, Arc<UserAdmissionState>>>,
}

impl AiAdmissionController {
    pub(crate) fn new(config: AiAdmissionConfig) -> Self {
        Self {
            config,
            global_streams: Arc::new(Semaphore::new(config.max_global_concurrent_sse())),
            users: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) const fn config(&self) -> AiAdmissionConfig {
        self.config
    }

    pub(crate) fn try_acquire(
        &self,
        user_id: UserId,
    ) -> Result<AiAdmissionLease, AiAdmissionError> {
        self.try_acquire_at(user_id, Instant::now())
    }

    fn try_acquire_at(
        &self,
        user_id: UserId,
        now: Instant,
    ) -> Result<AiAdmissionLease, AiAdmissionError> {
        let user_state = {
            let mut users = self
                .users
                .lock()
                .map_err(|_| AiAdmissionError::Unavailable)?;
            Arc::clone(users.entry(user_id).or_insert_with(|| {
                Arc::new(UserAdmissionState::new(
                    self.config.max_user_concurrent_sse(),
                ))
            }))
        };
        let user_permit = acquire(
            Arc::clone(&user_state.streams),
            AiAdmissionRejection::UserConcurrency,
        )?;
        let global_permit = acquire(
            Arc::clone(&self.global_streams),
            AiAdmissionRejection::GlobalConcurrency,
        )?;
        let ticket = user_state.reserve(now, self.config.requests_per_minute())?;
        Ok(AiAdmissionLease {
            user_state,
            ticket: Some(ticket),
            user_permit: Some(user_permit),
            global_permit: Some(global_permit),
        })
    }
}

fn acquire(
    semaphore: Arc<Semaphore>,
    rejection: AiAdmissionRejection,
) -> Result<OwnedSemaphorePermit, AiAdmissionError> {
    semaphore.try_acquire_owned().map_err(|error| match error {
        TryAcquireError::NoPermits => AiAdmissionError::Rejected(rejection),
        TryAcquireError::Closed => AiAdmissionError::Unavailable,
    })
}

struct UserAdmissionState {
    streams: Arc<Semaphore>,
    rate: Mutex<RateState>,
}

impl UserAdmissionState {
    fn new(max_concurrent_streams: usize) -> Self {
        Self {
            streams: Arc::new(Semaphore::new(max_concurrent_streams)),
            rate: Mutex::new(RateState::default()),
        }
    }

    fn reserve(&self, now: Instant, limit: u32) -> Result<u64, AiAdmissionError> {
        let mut rate = self
            .rate
            .lock()
            .map_err(|_| AiAdmissionError::Unavailable)?;
        while rate
            .reservations
            .front()
            .is_some_and(|reservation| now.saturating_duration_since(reservation.at) >= WINDOW)
        {
            rate.reservations.pop_front();
        }
        if rate.reservations.len() >= limit as usize {
            let oldest = rate
                .reservations
                .front()
                .expect("a full minute window contains an oldest reservation");
            let remaining = WINDOW.saturating_sub(now.saturating_duration_since(oldest.at));
            return Err(AiAdmissionError::Rejected(
                AiAdmissionRejection::MinuteRate {
                    retry_after_seconds: ceil_seconds(remaining),
                },
            ));
        }
        let ticket = rate.next_ticket;
        rate.next_ticket = rate.next_ticket.wrapping_add(1);
        rate.reservations
            .push_back(RateReservation { ticket, at: now });
        Ok(ticket)
    }

    fn cancel(&self, ticket: u64) {
        let Ok(mut rate) = self.rate.lock() else {
            return;
        };
        if let Some(index) = rate
            .reservations
            .iter()
            .position(|reservation| reservation.ticket == ticket)
        {
            rate.reservations.remove(index);
        }
    }
}

#[derive(Default)]
struct RateState {
    next_ticket: u64,
    reservations: VecDeque<RateReservation>,
}

struct RateReservation {
    ticket: u64,
    at: Instant,
}

pub(crate) struct AiAdmissionLease {
    user_state: Arc<UserAdmissionState>,
    ticket: Option<u64>,
    user_permit: Option<OwnedSemaphorePermit>,
    global_permit: Option<OwnedSemaphorePermit>,
}

impl AiAdmissionLease {
    pub(crate) fn commit(mut self) -> AiStreamPermit {
        self.ticket = None;
        AiStreamPermit {
            _user: self
                .user_permit
                .take()
                .expect("an uncommitted admission lease owns a user permit"),
            _global: self
                .global_permit
                .take()
                .expect("an uncommitted admission lease owns a global permit"),
        }
    }
}

impl Drop for AiAdmissionLease {
    fn drop(&mut self) {
        if let Some(ticket) = self.ticket.take() {
            self.user_state.cancel(ticket);
        }
    }
}

pub(crate) struct AiStreamPermit {
    _user: OwnedSemaphorePermit,
    _global: OwnedSemaphorePermit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AiAdmissionRejection {
    MinuteRate { retry_after_seconds: u64 },
    UserConcurrency,
    GlobalConcurrency,
    DailyQuota { retry_after_seconds: u64 },
}

impl AiAdmissionRejection {
    pub(crate) const fn retry_after_seconds(self) -> u64 {
        match self {
            Self::MinuteRate {
                retry_after_seconds,
            }
            | Self::DailyQuota {
                retry_after_seconds,
            } => retry_after_seconds,
            Self::UserConcurrency | Self::GlobalConcurrency => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AiAdmissionError {
    Rejected(AiAdmissionRejection),
    Unavailable,
}

fn ceil_seconds(duration: Duration) -> u64 {
    duration
        .as_secs()
        .saturating_add(u64::from(duration.subsec_nanos() != 0))
        .max(1)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use tjxy_common::UserId;
    use uuid::Uuid;

    use super::{AiAdmissionConfig, AiAdmissionController, AiAdmissionError, AiAdmissionRejection};

    fn user() -> UserId {
        UserId::from_uuid(Uuid::new_v4())
    }

    fn rejection(error: AiAdmissionError) -> AiAdmissionRejection {
        match error {
            AiAdmissionError::Rejected(rejection) => rejection,
            AiAdmissionError::Unavailable => panic!("admission state unexpectedly unavailable"),
        }
    }

    fn expect_rejection(
        result: Result<super::AiAdmissionLease, AiAdmissionError>,
    ) -> AiAdmissionError {
        match result {
            Ok(_) => panic!("admission unexpectedly succeeded"),
            Err(error) => error,
        }
    }

    #[test]
    fn configuration_rejects_zero_and_unsafe_caps() {
        assert!(AiAdmissionConfig::new(0, 2, 8, 100).is_err());
        assert!(AiAdmissionConfig::new(10, 0, 8, 100).is_err());
        assert!(AiAdmissionConfig::new(10, 2, 0, 100).is_err());
        assert!(AiAdmissionConfig::new(10, 2, 8, 0).is_err());
        assert!(AiAdmissionConfig::new(1_001, 2, 8, 100).is_err());
        assert!(AiAdmissionConfig::new(10, 101, 101, 100).is_err());
        assert!(AiAdmissionConfig::new(10, 2, 1_001, 100).is_err());
        assert!(AiAdmissionConfig::new(10, 2, 8, 100_001).is_err());
        assert!(AiAdmissionConfig::new(10, 9, 8, 100).is_err());
    }

    #[test]
    fn default_configuration_matches_the_admission_contract() {
        let config = AiAdmissionConfig::default();
        assert_eq!(config.requests_per_minute(), 10);
        assert_eq!(config.max_user_concurrent_sse(), 2);
        assert_eq!(config.max_global_concurrent_sse(), 8);
        assert_eq!(config.daily_quota(), 100);
    }

    #[test]
    fn eleventh_request_is_rejected_until_the_minute_window_expires() {
        let controller =
            AiAdmissionController::new(AiAdmissionConfig::new(10, 20, 20, 100).unwrap());
        let user_id = user();
        let now = Instant::now();
        for _ in 0..10 {
            drop(controller.try_acquire_at(user_id, now).unwrap().commit());
        }

        let rejection = rejection(expect_rejection(controller.try_acquire_at(user_id, now)));
        assert!(matches!(rejection, AiAdmissionRejection::MinuteRate { .. }));
        assert_eq!(rejection.retry_after_seconds(), 60);

        drop(
            controller
                .try_acquire_at(user_id, now + Duration::from_secs(60))
                .unwrap()
                .commit(),
        );
    }

    #[test]
    fn third_stream_for_one_user_is_rejected_and_drop_releases_capacity() {
        let controller = AiAdmissionController::new(AiAdmissionConfig::new(10, 2, 8, 100).unwrap());
        let user_id = user();
        let first = controller.try_acquire(user_id).unwrap().commit();
        let second = controller.try_acquire(user_id).unwrap().commit();

        assert!(matches!(
            rejection(expect_rejection(controller.try_acquire(user_id))),
            AiAdmissionRejection::UserConcurrency
        ));
        drop(first);
        let replacement = controller.try_acquire(user_id).unwrap().commit();
        drop((second, replacement));
    }

    #[test]
    fn global_capacity_is_shared_across_users_and_released_on_drop() {
        let controller = AiAdmissionController::new(AiAdmissionConfig::new(10, 2, 2, 100).unwrap());
        let first = controller.try_acquire(user()).unwrap().commit();
        let second = controller.try_acquire(user()).unwrap().commit();
        let waiting_user = user();

        assert!(matches!(
            rejection(expect_rejection(controller.try_acquire(waiting_user))),
            AiAdmissionRejection::GlobalConcurrency
        ));
        drop(first);
        let replacement = controller.try_acquire(waiting_user).unwrap().commit();
        drop((second, replacement));
    }

    #[test]
    fn dropping_uncommitted_lease_cancels_its_rate_reservation() {
        let controller = AiAdmissionController::new(AiAdmissionConfig::new(1, 1, 1, 100).unwrap());
        let user_id = user();
        drop(controller.try_acquire(user_id).unwrap());
        drop(controller.try_acquire(user_id).unwrap().commit());
    }
}
