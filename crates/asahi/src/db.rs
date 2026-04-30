use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbErr, EntityTrait, Schema,
};

use crate::entity::{comment, issue, issue_label, issue_relation};

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
    create_table(db, issue::Entity).await?;
    create_table(db, comment::Entity).await?;
    create_table(db, issue_label::Entity).await?;
    create_table(db, issue_relation::Entity).await?;
    Ok(())
}

async fn create_table<E>(db: &DatabaseConnection, entity: E) -> Result<(), DbErr>
where
    E: EntityTrait,
{
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);
    let mut statement = schema.create_table_from_entity(entity);
    statement.if_not_exists();
    db.execute(builder.build(&statement)).await?;
    Ok(())
}
