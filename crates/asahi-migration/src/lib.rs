pub use sea_orm_migration::prelude::*;

mod m20260430_000001_create_asahi_schema;
mod m20260430_000002_create_projects;
mod m20260501_000003_backfill_projects;
mod m20260501_000004_create_project_wiki;
mod m20260501_000005_remove_synthetic_default_project;

pub struct Migrator;

#[sea_orm_migration::async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260430_000001_create_asahi_schema::Migration),
            Box::new(m20260430_000002_create_projects::Migration),
            Box::new(m20260501_000003_backfill_projects::Migration),
            Box::new(m20260501_000004_create_project_wiki::Migration),
            Box::new(m20260501_000005_remove_synthetic_default_project::Migration),
        ]
    }
}
