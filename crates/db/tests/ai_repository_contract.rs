use std::sync::Arc;

use chrono::{Duration, NaiveDate, Utc};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use tjxy_credentials::{CredentialCipher, CredentialKey};
use tjxy_db::{
    AiConversationRepository, AiExecutionInput, AiExecutionOutcome, AiModelInput,
    AiReasoningEffort, AiSettingsRepository, AiSettingsRepositoryError, AiUsageRepository,
    AiUsageRepositoryError, AuthRepository, Migrator,
};
use tjxy_test_support::{reconnectable_test_database, test_database};
use uuid::Uuid;

const PROVIDER: &str = "openai-compatible";

fn cipher() -> CredentialCipher {
    CredentialCipher::new(CredentialKey::new(7, [7_u8; 32]).unwrap(), Vec::new()).unwrap()
}

fn models() -> Vec<AiModelInput> {
    vec![
        AiModelInput::new(Uuid::new_v4(), "movie-fast", "影视助手", true, true, 0),
        AiModelInput::new(Uuid::new_v4(), "movie-deep", "深度解析", true, false, 1)
            .with_reasoning_effort(AiReasoningEffort::High),
        AiModelInput::new(Uuid::new_v4(), "internal", "内部模型", false, false, 2),
    ]
}

async fn quota_database_connection(database_url: &str) -> DatabaseConnection {
    let mut options = ConnectOptions::new(database_url);
    options.max_connections(1).min_connections(1);
    let database = Database::connect(options).await.unwrap();
    if database.get_database_backend() == DbBackend::Sqlite {
        database
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA busy_timeout = 5000",
            ))
            .await
            .unwrap();
    }
    database
}

#[tokio::test]
async fn settings_store_encrypted_credentials_models_and_revision_fences() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let encryption = cipher();
    let credential_id = Uuid::new_v4();
    let sealed = encryption
        .seal_bound(credential_id, PROVIDER, b"secret-api-key")
        .unwrap();
    let model_inputs = models();
    let repository = AiSettingsRepository::new(&database);

    let created = repository
        .put(
            &sealed,
            true,
            "https://ai.example.test/v1",
            "Only discuss media.",
            80_000,
            8_000,
            &model_inputs,
            None,
        )
        .await
        .unwrap();

    assert_eq!(created.provider(), PROVIDER);
    assert!(created.enabled());
    assert_eq!(created.base_url(), "https://ai.example.test/v1");
    assert_eq!(created.system_prompt(), "Only discuss media.");
    assert_eq!(created.daily_total_token_limit(), 80_000);
    assert_eq!(created.daily_user_token_limit(), 8_000);
    assert_eq!(created.revision(), 1);
    assert_eq!(created.models().len(), 3);
    assert_eq!(created.visible_models().len(), 2);
    assert_eq!(created.default_model().unwrap().display_name(), "影视助手");
    assert_eq!(
        created.models()[0].reasoning_effort(),
        AiReasoningEffort::Off
    );
    assert_eq!(
        created.models()[1].reasoning_effort(),
        AiReasoningEffort::High
    );
    assert_eq!(
        encryption
            .open(
                created.credential_id(),
                created.provider(),
                created.envelope()
            )
            .unwrap()
            .as_slice(),
        b"secret-api-key"
    );

    let stale = repository
        .put(
            &sealed,
            false,
            "https://other.example.test/v1",
            "Changed",
            90_000,
            9_000,
            &model_inputs,
            Some(2),
        )
        .await;
    assert!(matches!(
        stale,
        Err(AiSettingsRepositoryError::RevisionConflict)
    ));

    let loaded = repository.get().await.unwrap().unwrap();
    assert_eq!(loaded.revision(), 1);
    assert_eq!(loaded.base_url(), "https://ai.example.test/v1");
}

#[tokio::test]
async fn settings_require_one_visible_default_and_unique_model_identifiers() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let sealed = cipher()
        .seal_bound(Uuid::new_v4(), PROVIDER, b"secret-api-key")
        .unwrap();
    let repository = AiSettingsRepository::new(&database);
    let no_default = vec![AiModelInput::new(
        Uuid::new_v4(),
        "movie-fast",
        "影视助手",
        true,
        false,
        0,
    )];

    assert!(matches!(
        repository
            .put(
                &sealed,
                true,
                "https://ai.example.test/v1",
                "Only discuss media.",
                0,
                0,
                &no_default,
                None,
            )
            .await,
        Err(AiSettingsRepositoryError::InvalidModels)
    ));

    let duplicate_id = Uuid::new_v4();
    let duplicates = vec![
        AiModelInput::new(duplicate_id, "first", "First", true, true, 0),
        AiModelInput::new(duplicate_id, "second", "Second", true, false, 1),
    ];
    assert!(matches!(
        repository
            .put(
                &sealed,
                true,
                "https://ai.example.test/v1",
                "Only discuss media.",
                0,
                0,
                &duplicates,
                None,
            )
            .await,
        Err(AiSettingsRepositoryError::InvalidModels)
    ));
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One ownership contract also verifies ordering across exchanges.
async fn conversations_are_user_scoped_and_exchanges_are_ordered() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let auth = AuthRepository::new(&database);
    let alice = auth
        .create_user(
            &tjxy_common::Username::parse("alice-ai").unwrap(),
            "test-only",
            true,
            false,
            Utc::now(),
        )
        .await
        .unwrap();
    let bob = auth
        .create_user(
            &tjxy_common::Username::parse("bob-ai").unwrap(),
            "test-only",
            true,
            false,
            Utc::now(),
        )
        .await
        .unwrap();
    let model_id = Uuid::new_v4();
    let expected_conversation_id = Uuid::new_v4();
    let repository = AiConversationRepository::new(&database);

    let conversation_id = repository
        .create_with_exchange_at(
            expected_conversation_id,
            alice.id(),
            model_id,
            "今晚看什么",
            "推荐一部科幻片",
            "可以看看《降临》。",
            &json!({"sources":[{"id":"arrival"}]}),
        )
        .await
        .unwrap();
    assert_eq!(conversation_id, expected_conversation_id);

    let loaded = repository
        .get(alice.id(), conversation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.title(), "今晚看什么");
    assert_eq!(loaded.model_id(), model_id);
    assert_eq!(loaded.messages().len(), 2);
    assert_eq!(loaded.messages()[0].role(), "user");
    assert_eq!(loaded.messages()[0].content(), "推荐一部科幻片");
    assert_eq!(loaded.messages()[1].role(), "assistant");
    assert_eq!(loaded.messages()[1].content(), "可以看看《降临》。");
    assert_eq!(
        loaded.messages()[1].metadata()["sources"][0]["id"],
        "arrival"
    );

    repository
        .append_exchange(
            alice.id(),
            conversation_id,
            "再来一部",
            "也可以看看《银翼杀手2049》。",
            &json!({}),
        )
        .await
        .unwrap();
    let loaded = repository
        .get(alice.id(), conversation_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        loaded
            .messages()
            .iter()
            .map(tjxy_db::AiMessageRecord::role)
            .collect::<Vec<_>>(),
        ["user", "assistant", "user", "assistant"]
    );
    assert_eq!(loaded.messages()[2].content(), "再来一部");
    assert_eq!(
        loaded.messages()[3].content(),
        "也可以看看《银翼杀手2049》。"
    );

    assert!(
        repository
            .get(bob.id(), conversation_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!repository.delete(bob.id(), conversation_id).await.unwrap());
    assert_eq!(repository.list(alice.id(), 20).await.unwrap().len(), 1);
    assert!(
        repository
            .delete(alice.id(), conversation_id)
            .await
            .unwrap()
    );
    assert!(
        repository
            .get(alice.id(), conversation_id)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn first_exchange_is_atomic_and_user_deletion_removes_ai_history() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let auth = AuthRepository::new(&database);
    let admin = auth
        .create_user(
            &tjxy_common::Username::parse("admin-ai-lifecycle").unwrap(),
            "test-only",
            true,
            false,
            Utc::now(),
        )
        .await
        .unwrap();
    let user = auth
        .create_user(
            &tjxy_common::Username::parse("user-ai-lifecycle").unwrap(),
            "test-only",
            false,
            false,
            Utc::now(),
        )
        .await
        .unwrap();
    let repository = AiConversationRepository::new(&database);

    assert!(
        repository
            .create_with_exchange(
                user.id(),
                Uuid::new_v4(),
                "Invalid exchange",
                "Question",
                "\u{0}",
                &json!({}),
            )
            .await
            .is_err()
    );
    assert!(repository.list(user.id(), 20).await.unwrap().is_empty());

    repository
        .create_with_exchange(
            user.id(),
            Uuid::new_v4(),
            "Tonight",
            "What should I watch?",
            "Try Arrival.",
            &json!({}),
        )
        .await
        .unwrap();
    auth.delete_user(user.id()).await.unwrap();
    assert!(repository.list(user.id(), 20).await.unwrap().is_empty());
    assert!(auth.get_user(admin.id()).await.unwrap().is_some());
}

#[tokio::test]
async fn ai_usage_analytics_tracks_unknown_tokens_and_rankings() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let auth = AuthRepository::new(&database);
    let alice = auth
        .create_user(
            &tjxy_common::Username::parse("alice-analytics").unwrap(),
            "test-only",
            false,
            false,
            Utc::now(),
        )
        .await
        .unwrap();
    let bob = auth
        .create_user(
            &tjxy_common::Username::parse("bob-analytics").unwrap(),
            "test-only",
            false,
            false,
            Utc::now(),
        )
        .await
        .unwrap();
    let model_id = Uuid::new_v4();
    let now = Utc::now();
    let repository = AiUsageRepository::new(&database);

    let empty = repository
        .analytics("2026-08-03", "2026-07-21", "2026-08-03", 10)
        .await
        .unwrap();
    assert_eq!(empty.summary.total_requests, 0);
    assert_eq!(empty.summary.successful_requests, 0);
    assert_eq!(empty.summary.failed_requests, 0);
    assert_eq!(empty.summary.total_tokens, Some(0));

    repository
        .record(
            &AiExecutionInput::new(
                alice.id(),
                model_id,
                "影视助手",
                "movie-fast",
                "2026-08-03",
                now - Duration::seconds(4),
                now,
                4_000,
                AiExecutionOutcome::Success,
            )
            .with_usage(100, 40),
        )
        .await
        .unwrap();
    repository
        .record(
            &AiExecutionInput::new(
                bob.id(),
                model_id,
                "影视助手",
                "movie-fast",
                "2026-08-03",
                now - Duration::seconds(2),
                now,
                2_000,
                AiExecutionOutcome::UpstreamTimeout,
            )
            .with_unknown_usage(),
        )
        .await
        .unwrap();

    let analytics = repository
        .analytics("2026-08-03", "2026-07-21", "2026-08-03", 10)
        .await
        .unwrap();
    assert_eq!(analytics.summary.total_requests, 2);
    assert_eq!(analytics.summary.active_users, 2);
    assert_eq!(analytics.summary.successful_requests, 1);
    assert_eq!(analytics.summary.failed_requests, 1);
    assert_eq!(analytics.summary.total_tokens, None);
    assert_eq!(analytics.summary.known_token_requests, 1);
    assert_eq!(analytics.users[0].username, "alice-analytics");
    assert_eq!(analytics.users[0].total_requests, 1);
    assert_eq!(analytics.models[0].display_name, "影视助手");
    assert_eq!(
        analytics.recent_failures[0].outcome,
        AiExecutionOutcome::UpstreamTimeout
    );
}

#[tokio::test]
async fn daily_quota_counts_requests_per_user_and_utc_day() {
    let database = test_database().await.unwrap();
    Migrator::up(&database, None).await.unwrap();
    let auth = AuthRepository::new(&database);
    let alice = auth
        .create_user(
            &tjxy_common::Username::parse("alice-daily-quota").unwrap(),
            "test-only",
            false,
            false,
            Utc::now(),
        )
        .await
        .unwrap();
    let bob = auth
        .create_user(
            &tjxy_common::Username::parse("bob-daily-quota").unwrap(),
            "test-only",
            false,
            false,
            Utc::now(),
        )
        .await
        .unwrap();
    let repository = AiUsageRepository::new(&database);
    let usage_day = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();

    assert!(
        repository
            .try_consume_daily_quota(alice.id(), usage_day, 2)
            .await
            .unwrap()
    );
    assert_eq!(
        repository
            .daily_quota_count(alice.id(), usage_day)
            .await
            .unwrap(),
        1
    );
    assert!(
        repository
            .try_consume_daily_quota(alice.id(), usage_day, 2)
            .await
            .unwrap()
    );
    assert_eq!(
        repository
            .daily_quota_count(alice.id(), usage_day)
            .await
            .unwrap(),
        2
    );
    assert!(
        !repository
            .try_consume_daily_quota(alice.id(), usage_day, 2)
            .await
            .unwrap()
    );
    assert_eq!(
        repository
            .daily_quota_count(alice.id(), usage_day)
            .await
            .unwrap(),
        2
    );

    let next_day = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
    assert!(
        repository
            .try_consume_daily_quota(alice.id(), next_day, 2)
            .await
            .unwrap()
    );
    assert!(
        repository
            .try_consume_daily_quota(bob.id(), usage_day, 2)
            .await
            .unwrap()
    );
    assert!(matches!(
        repository
            .try_consume_daily_quota(alice.id(), usage_day, 0)
            .await,
        Err(AiUsageRepositoryError::InvalidInput)
    ));
}

#[tokio::test]
async fn daily_quota_consumption_is_atomic_across_five_connections() {
    let fixture = reconnectable_test_database().await.unwrap();
    let database = fixture.connection();
    Migrator::up(database, None).await.unwrap();
    let user = AuthRepository::new(database)
        .create_user(
            &tjxy_common::Username::parse("concurrent-daily-quota").unwrap(),
            "test-only",
            false,
            false,
            Utc::now(),
        )
        .await
        .unwrap();
    let user_id = user.id();
    let usage_day = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
    let connections = Arc::new(vec![
        quota_database_connection(fixture.database_url()).await,
        quota_database_connection(fixture.database_url()).await,
        quota_database_connection(fixture.database_url()).await,
        quota_database_connection(fixture.database_url()).await,
        quota_database_connection(fixture.database_url()).await,
    ]);

    let calls = (0..10)
        .map(|index| {
            let connections = Arc::clone(&connections);
            tokio::spawn(async move {
                AiUsageRepository::new(&connections[index % connections.len()])
                    .try_consume_daily_quota(user_id, usage_day, 5)
                    .await
            })
        })
        .collect::<Vec<_>>();
    let mut successful = 0;
    for call in calls {
        successful += usize::from(call.await.unwrap().unwrap());
    }

    assert_eq!(successful, 5);
    assert_eq!(
        AiUsageRepository::new(database)
            .daily_quota_count(user_id, usage_day)
            .await
            .unwrap(),
        5
    );
}
