//! Loss-tolerant wakeups. Queue rows and lease fencing remain authoritative.

use std::{
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use sea_orm::{ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbBackend, DbErr};
use tokio::sync::watch;

const CHANNEL: &str = "tjxy_work_ready";
/// Storage inventory produced change-outbox events: only the projector and cache invalidation
/// consume them, so work claimers are not woken for every inventory page.
const OUTBOX_CHANNEL: &str = "tjxy_outbox_ready";
const INITIAL_DELAY: Duration = Duration::from_millis(200);
const MAX_DELAY: Duration = Duration::from_secs(5);

/// Set while the `PostgreSQL` listener is subscribed. It then delivers every hint, including this
/// process's own, so committers skip their local hint instead of waking every waiter twice.
static LISTENING: AtomicBool = AtomicBool::new(false);

fn notifications() -> &'static watch::Sender<()> {
    static NOTIFICATIONS: OnceLock<watch::Sender<()>> = OnceLock::new();
    NOTIFICATIONS.get_or_init(|| watch::channel(()).0)
}

fn outbox_notifications() -> &'static watch::Sender<()> {
    static NOTIFICATIONS: OnceLock<watch::Sender<()>> = OnceLock::new();
    NOTIFICATIONS.get_or_init(|| watch::channel(()).0)
}

/// Subscribe before claiming so a commit between an empty claim and waiting is retained.
pub struct WorkQueueWaiter {
    receiver: watch::Receiver<()>,
    outbox: Option<watch::Receiver<()>>,
    delay: Duration,
    jitter: u64,
}

impl Default for WorkQueueWaiter {
    fn default() -> Self {
        Self {
            receiver: notifications().subscribe(),
            outbox: None,
            delay: INITIAL_DELAY,
            jitter: u64::from_le_bytes(uuid::Uuid::new_v4().as_bytes()[..8].try_into().unwrap()),
        }
    }
}

impl WorkQueueWaiter {
    /// A waiter for change-outbox consumers: woken by work hints and by inventory outbox hints.
    #[must_use]
    pub fn outbox_consumer() -> Self {
        Self {
            outbox: Some(outbox_notifications().subscribe()),
            ..Self::default()
        }
    }

    /// Successful work resets backoff; the next claim still observes the database.
    pub fn reset(&mut self) {
        self.delay = INITIAL_DELAY;
        self.receiver.borrow_and_update();
        if let Some(outbox) = &mut self.outbox {
            outbox.borrow_and_update();
        }
    }

    /// Waits for committed work or a bounded polling fallback, including after lost notifications.
    pub async fn wait(&mut self) {
        self.jitter = self
            .jitter
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let delay = (self.delay + Duration::from_millis(self.jitter % 100)).min(MAX_DELAY);
        let outbox = async {
            match &mut self.outbox {
                Some(outbox) => outbox.changed().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            _ = self.receiver.changed() => self.delay = INITIAL_DELAY,
            _ = outbox => self.delay = INITIAL_DELAY,
            () = tokio::time::sleep(delay) => self.delay = (self.delay * 2).min(MAX_DELAY),
        }
    }
}

/// Commits work that other workers may now claim, and wakes them.
pub(crate) async fn commit_and_notify(transaction: DatabaseTransaction) -> Result<(), DbErr> {
    commit_with_hint(transaction, CHANNEL, notifications()).await
}

/// Commits a transaction that made work claimable (an enqueue or a released dependency) and
/// wakes idle workers. For callers outside this crate that assemble their own transaction.
///
/// # Errors
/// Returns the notification or commit failure; the transaction is not committed on error.
pub async fn commit_and_wake_workers(transaction: DatabaseTransaction) -> Result<(), DbErr> {
    commit_and_notify(transaction).await
}

/// Commits change-outbox events without waking work claimers.
pub(crate) async fn commit_and_notify_outbox(
    transaction: DatabaseTransaction,
) -> Result<(), DbErr> {
    commit_with_hint(transaction, OUTBOX_CHANNEL, outbox_notifications()).await
}

async fn commit_with_hint(
    transaction: DatabaseTransaction,
    channel: &str,
    local: &watch::Sender<()>,
) -> Result<(), DbErr> {
    let backend = transaction.get_database_backend();
    // PostgreSQL delivers this hint only when the surrounding transaction commits.
    if backend == DbBackend::Postgres {
        transaction
            .execute_unprepared(&format!("SELECT pg_notify('{channel}', '')"))
            .await?;
    }
    transaction.commit().await?;
    if backend != DbBackend::Postgres || !LISTENING.load(Ordering::Acquire) {
        local.send_replace(());
    }
    Ok(())
}

/// Clears the listening flag when the listener stops, however it stops.
struct ListeningGuard;

impl Drop for ListeningGuard {
    fn drop(&mut self) {
        LISTENING.store(false, Ordering::Release);
    }
}

/// Bridges `PostgreSQL` notifications to local workers. Other backends use local hints and polling.
///
/// # Errors
/// Returns connection/listener failures; callers should reconnect with backoff while polling continues.
pub async fn listen_for_work(database_url: &str) -> Result<(), DbErr> {
    // A dedicated connection keeps the permanent LISTEN session out of the request pool.
    let mut listener = sea_orm::sqlx::postgres::PgListener::connect(database_url)
        .await
        .map_err(|_| DbErr::Custom("work notification connection failed".to_owned()))?;
    listener
        .listen_all([CHANNEL, OUTBOX_CHANNEL])
        .await
        .map_err(|_| DbErr::Custom("work notification subscription failed".to_owned()))?;
    let _listening = ListeningGuard;
    LISTENING.store(true, Ordering::Release);
    // Hints committed while no listener was subscribed may have been missed.
    notifications().send_replace(());
    outbox_notifications().send_replace(());
    loop {
        let notification = listener
            .recv()
            .await
            .map_err(|_| DbErr::Custom("work notification connection interrupted".to_owned()))?;
        if notification.channel() == OUTBOX_CHANNEL {
            outbox_notifications().send_replace(());
        } else {
            notifications().send_replace(());
        }
    }
}

/// Best-effort hint following a single-statement autocommit transition. Polling covers a lost hint.
pub(crate) async fn notify_after_commit(database: &DatabaseConnection) {
    if database.get_database_backend() != DbBackend::Postgres || !LISTENING.load(Ordering::Acquire)
    {
        notifications().send_replace(());
    }
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
            outbox: None,
            delay: INITIAL_DELAY,
            jitter: 0,
        };
        waiter.wait().await;
        assert_eq!(waiter.delay, INITIAL_DELAY * 2);
        drop(sender);
        waiter.reset();
        assert_eq!(waiter.delay, INITIAL_DELAY);
    }

    #[tokio::test]
    async fn outbox_hints_wake_only_outbox_consumers() {
        let (work, work_receiver) = watch::channel(());
        let (outbox, outbox_receiver) = watch::channel(());
        let mut claimer = WorkQueueWaiter {
            receiver: work_receiver.clone(),
            outbox: None,
            delay: MAX_DELAY,
            jitter: 0,
        };
        let mut consumer = WorkQueueWaiter {
            receiver: work_receiver,
            outbox: Some(outbox_receiver),
            delay: MAX_DELAY,
            jitter: 0,
        };
        outbox.send_replace(());
        assert!(
            tokio::time::timeout(Duration::from_millis(100), claimer.wait())
                .await
                .is_err(),
            "an inventory page must not wake work claimers"
        );
        tokio::time::timeout(Duration::from_millis(100), consumer.wait())
            .await
            .unwrap();
        consumer.reset();
        work.send_replace(());
        tokio::time::timeout(Duration::from_millis(100), consumer.wait())
            .await
            .unwrap();
    }
}
