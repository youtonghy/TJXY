use std::time::Duration;
use thiserror::Error;

/// One cumulative allowance for the descriptor, metadata requests, sparse reads and retries.
#[derive(Clone, Copy, Debug)]
pub struct ProbeLimits {
    pub max_requests: u32,
    pub max_bytes: u64,
    pub timeout: Duration,
}
impl Default for ProbeLimits {
    fn default() -> Self {
        Self {
            max_requests: 32,
            max_bytes: 64 * 1024 * 1024,
            timeout: Duration::from_secs(45),
        }
    }
}

pub(crate) struct ProbeBudget {
    pub(crate) limits: ProbeLimits,
    pub(crate) deadline: tokio::time::Instant,
    pub(crate) requests: u32,
    pub(crate) bytes: u64,
    pub(crate) gap_reads: u32,
    reserved_bytes: u64,
}
impl ProbeBudget {
    pub(crate) fn new(limits: ProbeLimits) -> Self {
        Self {
            deadline: tokio::time::Instant::now() + limits.timeout,
            limits,
            requests: 0,
            bytes: 0,
            gap_reads: 0,
            reserved_bytes: 0,
        }
    }
    pub(crate) fn request(&mut self, bytes: u64) -> Result<(), ProbeBudgetError> {
        if tokio::time::Instant::now() >= self.deadline {
            return Err(ProbeBudgetError::Time);
        }
        if self.requests >= self.limits.max_requests {
            return Err(ProbeBudgetError::Requests);
        }
        if bytes > self.limits.max_bytes.saturating_sub(self.reserved_bytes) {
            return Err(ProbeBudgetError::Bytes);
        }
        self.requests += 1;
        self.reserved_bytes += bytes;
        Ok(())
    }
    pub(crate) fn received(&mut self, bytes: usize) -> Result<(), ProbeBudgetError> {
        self.bytes = self.bytes.saturating_add(bytes as u64);
        if self.bytes > self.limits.max_bytes {
            return Err(ProbeBudgetError::Bytes);
        }
        Ok(())
    }
}
#[derive(Debug, Error)]
pub enum ProbeBudgetError {
    #[error("probe cumulative request budget exhausted")]
    Requests,
    #[error("probe cumulative byte budget exhausted")]
    Bytes,
    #[error("probe elapsed time budget exhausted")]
    Time,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn retries_share_the_same_byte_and_request_allowance() {
        let mut budget = ProbeBudget::new(ProbeLimits {
            max_requests: 3,
            max_bytes: 10,
            timeout: Duration::from_secs(1),
        });
        budget.request(0).unwrap();
        budget.request(6).unwrap();
        budget.received(6).unwrap();
        assert!(matches!(budget.request(5), Err(ProbeBudgetError::Bytes)));
        budget.request(4).unwrap();
        budget.received(4).unwrap();
        assert!(matches!(budget.request(0), Err(ProbeBudgetError::Requests)));
        assert_eq!(budget.bytes, 10);
    }
    #[tokio::test]
    async fn expired_budget_does_not_start_another_request() {
        let mut budget = ProbeBudget::new(ProbeLimits {
            timeout: Duration::ZERO,
            ..ProbeLimits::default()
        });
        assert!(matches!(budget.request(0), Err(ProbeBudgetError::Time)));
        assert_eq!(budget.requests, 0);
    }
}
