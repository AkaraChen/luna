use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Projects::Table)
                    .if_not_exists()
                    .col(string(Projects::Id).primary_key().take())
                    .col(string(Projects::Slug))
                    .col(string(Projects::Name))
                    .col(text_null(Projects::Description))
                    .col(big_integer_null(Projects::Priority))
                    .col(string(Projects::State))
                    .col(string_null(Projects::Url))
                    .col(timestamp_with_time_zone(Projects::CreatedAt))
                    .col(timestamp_with_time_zone(Projects::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        if !manager
            .has_column(Issues::Table.to_string(), Issues::ProjectId.to_string())
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(Issues::Table)
                        .add_column(string_null(Issues::ProjectId))
                        .to_owned(),
                )
                .await?;
        }

        create_unique_index(
            manager,
            "idx_projects_slug",
            Projects::Table,
            Projects::Slug,
        )
        .await?;
        create_index(
            manager,
            "idx_issues_project_id",
            Issues::Table,
            Issues::ProjectId,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_index(manager, "idx_issues_project_id", Issues::Table).await?;
        drop_index(manager, "idx_projects_slug", Projects::Table).await?;

        if manager
            .has_column(Issues::Table.to_string(), Issues::ProjectId.to_string())
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(Issues::Table)
                        .drop_column(Issues::ProjectId)
                        .to_owned(),
                )
                .await?;
        }

        manager
            .drop_table(Table::drop().table(Projects::Table).to_owned())
            .await
    }
}

async fn create_index<T, C>(
    manager: &SchemaManager<'_>,
    name: &str,
    table: T,
    column: C,
) -> Result<(), DbErr>
where
    T: Iden + 'static,
    C: Iden + 'static,
{
    if manager.has_index(table.to_string(), name).await? {
        return Ok(());
    }

    manager
        .create_index(
            Index::create()
                .name(name)
                .table(table)
                .col(column)
                .to_owned(),
        )
        .await
}

async fn create_unique_index<T, C>(
    manager: &SchemaManager<'_>,
    name: &str,
    table: T,
    column: C,
) -> Result<(), DbErr>
where
    T: Iden + 'static,
    C: Iden + 'static,
{
    if manager.has_index(table.to_string(), name).await? {
        return Ok(());
    }

    manager
        .create_index(
            Index::create()
                .name(name)
                .table(table)
                .col(column)
                .unique()
                .to_owned(),
        )
        .await
}

async fn drop_index<T>(manager: &SchemaManager<'_>, name: &str, table: T) -> Result<(), DbErr>
where
    T: Iden + 'static,
{
    if !manager.has_index(table.to_string(), name).await? {
        return Ok(());
    }

    manager
        .drop_index(Index::drop().name(name).table(table).to_owned())
        .await
}

#[derive(DeriveIden)]
enum Projects {
    Table,
    Id,
    Slug,
    Name,
    Description,
    Priority,
    State,
    Url,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Issues {
    Table,
    ProjectId,
}
