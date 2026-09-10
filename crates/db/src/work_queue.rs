//! Loss-tolerant wakeups. Queue rows and lease fencing remain authoritative.

use std::{sync::OnceLock, time::Duration};

use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, DbErr};
use tokio::sync::watch;

const CHANNEL: &str = "tjxy_work_ready";
const INITIAL_DELAY: Duration = Duration::from_millis(200);
const MAX_DELAY: Duration = Duration::from_secs(5);

fn notifications() -> &'static watch::Sender<()> {
    static NOTIFICATIONS: OnceLock<watch::Sender<()>> = OnceLock::new();
    NOTIFICATIONS.get_or_init(|| watch::channel(()).0)
}

/// Subscribe before claiming so a commit between an empty claim and waiting is retained.
pub struct WorkQueueWaiter {
    receiver: watch::Receiver<()>,
    delay: Duration,
    jitter: u64,
}

impl Default for WorkQueueWaiter {
    fn default() -> Self {
        Self {
            receiver: notifications().subscribe(),
            delay: INITIAL_DELAY,
            jitter: u64::from_le_bytes(uuid::Uuid::new_v4().as_bytes()[..8].try_into().unwrap()),
        }
    }
}

impl WorkQueueWaiter {
    /// Successful work resets backoff; the next claim still observes the database.
    pub fn reset(&mut self) {
        self.delay = INITIAL_DELAY;
        self.receiver.borrow_and_update();
    }

    /// Waits for committed work or a bounded polling fallback, including after lost notifications.
    pub async fn wait(&mut self) {
        self.jitter = self
            .jitter
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let delay = (self.delay + Duration::from_millis(self.jitter % 100)).min(MAX_DELAY);
        tokio::select! {
            _ = self.receiver.changed() => self.delay = INITIAL_DELAY,
            () = tokio::time::sleep(delay) => self.delay = (self.delay * 2).min(MAX_DELAY),
        }
    }
}

pub(crate) async fn commit_and_notify(transaction: DatabaseTransaction) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    // PostgreSQL delivers this hint only when the surrounding transaction commits.
    if backend == DbBackend::Postgres {
        transaction
            .execute_unprepared("SELECT pg_notify('tjxy_work_ready', '')")
            .await?;
    }
    transaction.commit().await?;
    notifications().send_replace(());
    Ok(())
}

/// Bridges `PostgreSQL` notifications to local workers. Other backends use local hints and polling.
///
/// # Errors
/// Returns connection/listener failures; callers should reconnect with backoff while polling continues.
pub async fn listen_for_work(database: &DatabaseConnection) -> Result<(), DbErr> {
    if database.get_database_backend() != DbBackend::Postgres {
        return Ok(());
    }
    let mut listener =
        sea_orm::sqlx::postgres::PgListener::connect_with(database.get_postgres_connection_pool())
            .await
            .map_err(|_| DbErr::Custom("work notification connection failed".to_owned()))?;
    listener
        .listen(CHANNEL)
        .await
        .map_err(|_| DbErr::Custom("work notification subscription failed".to_owned()))?;
    notifications().send_replace(());
    loop {
        listener
            .recv()
            .await
            .map_err(|_| DbErr::Custom("work notification connection interrupted".to_owned()))?;
        notifications().send_replace(());
    }
}

/// Best-effort hint following a single-statement autocommit transition. Polling covers a lost hint.
pub(crate) async fn notify_after_commit(database: &DatabaseConnection) {
    notifications().send_replace(());
    if database.get_database_backend() == DbBackend::Postgres
        && database
            .execute_unprepared("SELECT pg_notify('tjxy_work_ready', '')")
            .await
            .is_err()
    {
        tracing::warn!("post-commit work notification failed; polling remains enabled");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn notification_between_claim_and_wait_is_not_lost() {
        let mut waiter = WorkQueueWaiter {
            delay: MAX_DELAY,
            ..WorkQueueWaiter::default()
        };
        notifications().send_replace(());
        tokio::time::timeout(Duration::from_millis(100), waiter.wait())
            .await
            .unwrap();
        assert_eq!(waiter.delay, INITIAL_DELAY);
    }

    #[tokio::test]
    async fn polling_survives_without_notifications() {
        let (sender, receiver) = watch::channel(());
        let mut waiter = WorkQueueWaiter {
            receiver,
            delay: INITIAL_DELAY,
            jitter: 0,
        };
        waiter.wait().await;
        assert_eq!(waiter.delay, INITIAL_DELAY * 2);
        drop(sender);
        waiter.reset();
        assert_eq!(waiter.delay, INITIAL_DELAY);
    }
}
