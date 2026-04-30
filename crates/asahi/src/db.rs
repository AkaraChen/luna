use asahi_migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

const DEFAULT_DATABASE_URL: &str = "sqlite://asahi.db?mode=rwc";

pub fn database_url_from_env() -> String {
    std::env::var("ASAHI_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string())
}

pub async fn connect_and_setup(database_url: &str) -> Result<DatabaseConnection, DbErr> {
    let mut options = ConnectOptions::new(database_url.to_string());
    options.sqlx_logging(false);

    let db = Database::connect(options).await?;
    setup_schema(&db).await?;
    Ok(db)
}

async fn setup_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    Migrator::up(db, None).await
}
