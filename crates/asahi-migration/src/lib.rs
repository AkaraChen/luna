pub use sea_orm_migration::prelude::*;

mod m20260430_000001_create_asahi_schema;

pub struct Migrator;

#[sea_orm_migration::async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(m20260430_000001_create_asahi_schema::Migration)]
    }
}
