use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(WikiNodes::Table)
                    .if_not_exists()
                    .col(string(WikiNodes::Id).primary_key().take())
                    .col(string(WikiNodes::ProjectId))
                    .col(string_null(WikiNodes::ParentId))
                    .col(string(WikiNodes::Kind))
                    .col(string(WikiNodes::Title))
                    .col(string(WikiNodes::Slug))
                    .col(text_null(WikiNodes::Content))
                    .col(string_null(WikiNodes::CurrentVersionId))
                    .col(timestamp_with_time_zone(WikiNodes::CreatedAt))
                    .col(timestamp_with_time_zone(WikiNodes::UpdatedAt))
                    .col(timestamp_with_time_zone_null(WikiNodes::DeletedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(WikiPageVersions::Table)
                    .if_not_exists()
                    .col(string(WikiPageVersions::Id).primary_key().take())
                    .col(string(WikiPageVersions::PageId))
                    .col(big_integer(WikiPageVersions::Version))
                    .col(string(WikiPageVersions::Title))
                    .col(text(WikiPageVersions::Content))
                    .col(string(WikiPageVersions::ActorKind))
                    .col(string_null(WikiPageVersions::ActorId))
                    .col(text_null(WikiPageVersions::Summary))
                    .col(timestamp_with_time_zone(WikiPageVersions::CreatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(WikiAudits::Table)
                    .if_not_exists()
                    .col(string(WikiAudits::Id).primary_key().take())
                    .col(string(WikiAudits::ProjectId))
                    .col(string(WikiAudits::NodeId))
                    .col(string_null(WikiAudits::VersionId))
                    .col(string(WikiAudits::Action))
                    .col(string(WikiAudits::ActorKind))
                    .col(string_null(WikiAudits::ActorId))
                    .col(text_null(WikiAudits::Summary))
                    .col(timestamp_with_time_zone(WikiAudits::CreatedAt))
                    .to_owned(),
            )
            .await?;

        create_index(
            manager,
            "idx_wiki_nodes_project_parent",
            WikiNodes::Table,
            vec![WikiNodes::ProjectId, WikiNodes::ParentId],
        )
        .await?;
        create_index(
            manager,
            "idx_wiki_nodes_parent",
            WikiNodes::Table,
            vec![WikiNodes::ParentId],
        )
        .await?;
        create_index(
            manager,
            "idx_wiki_nodes_deleted_at",
            WikiNodes::Table,
            vec![WikiNodes::DeletedAt],
        )
        .await?;
        create_unique_index(
            manager,
            "idx_wiki_page_versions_page_version",
            WikiPageVersions::Table,
            vec![WikiPageVersions::PageId, WikiPageVersions::Version],
        )
        .await?;
        create_index(
            manager,
            "idx_wiki_audits_project_created",
            WikiAudits::Table,
            vec![WikiAudits::ProjectId, WikiAudits::CreatedAt],
        )
        .await?;
        create_index(
            manager,
            "idx_wiki_audits_node_created",
            WikiAudits::Table,
            vec![WikiAudits::NodeId, WikiAudits::CreatedAt],
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_index(manager, "idx_wiki_audits_node_created", WikiAudits::Table).await?;
        drop_index(
            manager,
            "idx_wiki_audits_project_created",
            WikiAudits::Table,
        )
        .await?;
        drop_index(
            manager,
            "idx_wiki_page_versions_page_version",
            WikiPageVersions::Table,
        )
        .await?;
        drop_index(manager, "idx_wiki_nodes_deleted_at", WikiNodes::Table).await?;
        drop_index(manager, "idx_wiki_nodes_parent", WikiNodes::Table).await?;
        drop_index(manager, "idx_wiki_nodes_project_parent", WikiNodes::Table).await?;

        manager
            .drop_table(Table::drop().table(WikiAudits::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(WikiPageVersions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(WikiNodes::Table).to_owned())
            .await
    }
}

async fn create_index<T>(
    manager: &SchemaManager<'_>,
    name: &str,
    table: T,
    columns: Vec<impl Iden + 'static>,
) -> Result<(), DbErr>
where
    T: Iden + 'static,
{
    if manager.has_index(table.to_string(), name).await? {
        return Ok(());
    }

    let mut index = Index::create();
    index.name(name).table(table);
    for column in columns {
        index.col(column);
    }
    manager.create_index(index.to_owned()).await
}

async fn create_unique_index<T>(
    manager: &SchemaManager<'_>,
    name: &str,
    table: T,
    columns: Vec<impl Iden + 'static>,
) -> Result<(), DbErr>
where
    T: Iden + 'static,
{
    if manager.has_index(table.to_string(), name).await? {
        return Ok(());
    }

    let mut index = Index::create();
    index.name(name).table(table).unique();
    for column in columns {
        index.col(column);
    }
    manager.create_index(index.to_owned()).await
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
enum WikiNodes {
    Table,
    Id,
    ProjectId,
    ParentId,
    Kind,
    Title,
    Slug,
    Content,
    CurrentVersionId,
    CreatedAt,
    UpdatedAt,
    DeletedAt,
}

#[derive(DeriveIden)]
enum WikiPageVersions {
    Table,
    Id,
    PageId,
    Version,
    Title,
    Content,
    ActorKind,
    ActorId,
    Summary,
    CreatedAt,
}

#[derive(DeriveIden)]
enum WikiAudits {
    Table,
    Id,
    ProjectId,
    NodeId,
    VersionId,
    Action,
    ActorKind,
    ActorId,
    Summary,
    CreatedAt,
}
