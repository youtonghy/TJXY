use std::{future::Future, time::Duration};

/// Keep polling work while renewal waits for the database. In particular, the
/// work may own `SQLite`'s sole connection and must be allowed to commit before
/// renewal can acquire it. A failed renewal still cancels further execution.
pub(super) async fn run<T, Error, Renewal>(
    work: impl Future<Output = Result<T, Error>>,
    mut renew: impl FnMut() -> Renewal,
    period: Duration,
    lost_lease: Error,
) -> Result<T, Error>
where
    Renewal: Future<Output = bool>,
{
    let maintain_lease = async {
        let mut timer = tokio::time::interval(period);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        timer.tick().await;
        loop {
            timer.tick().await;
            if !renew().await {
                return lost_lease;
            }
        }
    };
    tokio::select! {
        result = work => result,
        error = maintain_lease => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, ConnectionTrait, Database, TransactionTrait};
    use std::sync::Arc;
    use tokio::sync::Notify;

    #[tokio::test]
    async fn renewal_does_not_pause_work_holding_the_only_sqlite_connection() {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.max_connections(1).min_connections(1);
        let database = Database::connect(options).await.unwrap();
        let transaction = database.begin().await.unwrap();
        let attempted = Arc::new(Notify::new());
        let renewed = Arc::new(Notify::new());
        let work = async {
            attempted.notified().await;
            // Renewal is now waiting for the connection held by this work.
            transaction.commit().await.unwrap();
            renewed.notified().await;
            Ok::<_, &'static str>(())
        };
        let renew = || async {
            attempted.notify_one();
            let result = tokio::time::timeout(
                Duration::from_millis(100),
                database.execute_unprepared("SELECT 1"),
            )
            .await;
            if matches!(result, Ok(Ok(_))) {
                renewed.notify_one();
                true
            } else {
                false
            }
        };
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            run(work, renew, Duration::from_millis(1), "lost lease"),
        )
        .await
        .unwrap();
        assert_eq!(result, Ok(()));
    }

    #[tokio::test]
    async fn failed_renewal_stops_unfinished_work() {
        let result = run(
            std::future::pending::<Result<(), &str>>(),
            || async { false },
            Duration::from_millis(1),
            "lost lease",
        )
        .await;
        assert_eq!(result, Err("lost lease"));
    }
}
