mod m20260716_000001_phase_zero_schema;
mod m20260717_000002_complete_phase_zero_schema;
mod m20260717_000003_outbox_claim_indexes;
mod m20260717_000004_auth_sessions;

use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260716_000001_phase_zero_schema::Migration),
            Box::new(m20260717_000002_complete_phase_zero_schema::Migration),
            Box::new(m20260717_000003_outbox_claim_indexes::Migration),
            Box::new(m20260717_000004_auth_sessions::Migration),
        ]
    }
}
