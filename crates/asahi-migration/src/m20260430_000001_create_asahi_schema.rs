use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Issues::Table)
                    .if_not_exists()
                    .col(string(Issues::Id).primary_key().take())
                    .col(string(Issues::Identifier))
                    .col(string(Issues::ProjectSlug))
                    .col(string(Issues::TeamKey))
                    .col(big_integer(Issues::Number))
                    .col(string(Issues::Title))
                    .col(text_null(Issues::Description))
                    .col(big_integer_null(Issues::Priority))
                    .col(string(Issues::State))
                    .col(string_null(Issues::BranchName))
                    .col(string_null(Issues::Url))
                    .col(string_null(Issues::AssigneeId))
                    .col(timestamp_with_time_zone(Issues::CreatedAt))
                    .col(timestamp_with_time_zone(Issues::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Comments::Table)
                    .if_not_exists()
                    .col(string(Comments::Id).primary_key().take())
                    .col(string(Comments::IssueId))
                    .col(text(Comments::Body))
                    .col(timestamp_with_time_zone(Comments::CreatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(IssueLabels::Table)
                    .if_not_exists()
                    .col(string(IssueLabels::Id).primary_key().take())
                    .col(string(IssueLabels::IssueId))
                    .col(string(IssueLabels::Name))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(IssueRelations::Table)
                    .if_not_exists()
                    .col(string(IssueRelations::Id).primary_key().take())
                    .col(string(IssueRelations::IssueId))
                    .col(string(IssueRelations::BlockedByIssueId))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Notifications::Table)
                    .if_not_exists()
                    .col(string(Notifications::Id).primary_key().take())
                    .col(string(Notifications::Kind))
                    .col(string_null(Notifications::IssueId))
                    .col(string_null(Notifications::RecipientId))
                    .col(string_null(Notifications::ActorId))
                    .col(string(Notifications::Title))
                    .col(text_null(Notifications::Body))
                    .col(timestamp_with_time_zone_null(Notifications::ReadAt))
                    .col(timestamp_with_time_zone_null(Notifications::ArchivedAt))
                    .col(timestamp_with_time_zone(Notifications::CreatedAt))
                    .col(timestamp_with_time_zone(Notifications::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        create_index(
            manager,
            "idx_issues_identifier",
            Issues::Table,
            Issues::Identifier,
        )
        .await?;
        create_index(
            manager,
            "idx_comments_issue_id",
            Comments::Table,
            Comments::IssueId,
        )
        .await?;
        create_index(
            manager,
            "idx_issue_labels_issue_id",
            IssueLabels::Table,
            IssueLabels::IssueId,
        )
        .await?;
        create_index(
            manager,
            "idx_issue_relations_issue_id",
            IssueRelations::Table,
            IssueRelations::IssueId,
        )
        .await?;
        create_index(
            manager,
            "idx_notifications_created_at",
            Notifications::Table,
            Notifications::CreatedAt,
        )
        .await?;
        create_index(
            manager,
            "idx_notifications_archived_at",
            Notifications::Table,
            Notifications::ArchivedAt,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Notifications::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(IssueRelations::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(IssueLabels::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Comments::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Issues::Table).to_owned())
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

#[derive(DeriveIden)]
enum Issues {
    Table,
    Id,
    Identifier,
    ProjectSlug,
    TeamKey,
    Number,
    Title,
    Description,
    Priority,
    State,
    BranchName,
    Url,
    AssigneeId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Comments {
    Table,
    Id,
    IssueId,
    Body,
    CreatedAt,
}

#[derive(DeriveIden)]
enum IssueLabels {
    Table,
    Id,
    IssueId,
    Name,
}

#[derive(DeriveIden)]
enum IssueRelations {
    Table,
    Id,
    IssueId,
    BlockedByIssueId,
}

#[derive(DeriveIden)]
enum Notifications {
    Table,
    Id,
    Kind,
    IssueId,
    RecipientId,
    ActorId,
    Title,
    Body,
    ReadAt,
    ArchivedAt,
    CreatedAt,
    UpdatedAt,
}
