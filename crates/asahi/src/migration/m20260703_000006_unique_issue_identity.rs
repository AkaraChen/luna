use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        dedupe_issue_identity(manager).await?;
        create_unique_index(
            manager,
            "uq_issues_team_key_number",
            [Issues::TeamKey, Issues::Number],
        )
        .await?;
        create_unique_index(manager, "uq_issues_identifier", [Issues::Identifier]).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_index(manager, "uq_issues_identifier").await?;
        drop_index(manager, "uq_issues_team_key_number").await
    }
}

async fn dedupe_issue_identity(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute_unprepared(
            r#"
WITH duplicate_numbers AS (
    SELECT id, team_key,
           ROW_NUMBER() OVER (PARTITION BY team_key, number ORDER BY created_at, id) AS duplicate_rank
    FROM issues
),
to_move AS (
    SELECT id, team_key,
           (SELECT COALESCE(MAX(number), 0) FROM issues existing WHERE existing.team_key = duplicate_numbers.team_key)
           + ROW_NUMBER() OVER (PARTITION BY team_key ORDER BY id) AS new_number
    FROM duplicate_numbers
    WHERE duplicate_rank > 1
)
UPDATE issues
SET number = (SELECT new_number FROM to_move WHERE to_move.id = issues.id),
    identifier = team_key || '-' || (SELECT new_number FROM to_move WHERE to_move.id = issues.id),
    url = '/api/issues/' || team_key || '-' || (SELECT new_number FROM to_move WHERE to_move.id = issues.id)
WHERE id IN (SELECT id FROM to_move);
"#,
        )
        .await?;

    manager
        .get_connection()
        .execute_unprepared(
            r#"
WITH duplicate_identifiers AS (
    SELECT id, team_key,
           ROW_NUMBER() OVER (PARTITION BY identifier ORDER BY created_at, id) AS duplicate_rank
    FROM issues
),
to_move AS (
    SELECT id, team_key,
           (SELECT COALESCE(MAX(number), 0) FROM issues existing WHERE existing.team_key = duplicate_identifiers.team_key)
           + ROW_NUMBER() OVER (PARTITION BY team_key ORDER BY id) AS new_number
    FROM duplicate_identifiers
    WHERE duplicate_rank > 1
)
UPDATE issues
SET number = (SELECT new_number FROM to_move WHERE to_move.id = issues.id),
    identifier = team_key || '-' || (SELECT new_number FROM to_move WHERE to_move.id = issues.id),
    url = '/api/issues/' || team_key || '-' || (SELECT new_number FROM to_move WHERE to_move.id = issues.id)
WHERE id IN (SELECT id FROM to_move);
"#,
        )
        .await?;
    Ok(())
}

async fn create_unique_index<const N: usize>(
    manager: &SchemaManager<'_>,
    name: &str,
    columns: [Issues; N],
) -> Result<(), DbErr> {
    if manager.has_index(Issues::Table.to_string(), name).await? {
        return Ok(());
    }

    let mut index = Index::create();
    index.name(name).table(Issues::Table).unique();
    for column in columns {
        index.col(column);
    }
    manager.create_index(index.to_owned()).await
}

async fn drop_index(manager: &SchemaManager<'_>, name: &str) -> Result<(), DbErr> {
    if !manager.has_index(Issues::Table.to_string(), name).await? {
        return Ok(());
    }

    manager
        .drop_index(Index::drop().name(name).table(Issues::Table).to_owned())
        .await
}

#[derive(Clone, Copy, DeriveIden)]
enum Issues {
    Table,
    Identifier,
    TeamKey,
    Number,
}
