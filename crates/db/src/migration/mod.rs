mod m20260716_000001_phase_zero_schema;

use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20260716_000001_phase_zero_schema::Migration)]
    }
}
