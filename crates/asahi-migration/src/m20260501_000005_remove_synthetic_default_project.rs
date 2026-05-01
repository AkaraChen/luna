use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            r#"
            UPDATE issues
            SET project_id = NULL
            WHERE project_id IN (
                SELECT id
                FROM projects
                WHERE lower(slug) = 'default'
                  AND name = 'default'
                  AND description IS NULL
                  AND priority IS NULL
                  AND state = 'Backlog'
            )
            "#,
        )
        .await?;

        db.execute_unprepared(
            r#"
            DELETE FROM projects
            WHERE lower(slug) = 'default'
              AND name = 'default'
              AND description IS NULL
              AND priority IS NULL
              AND state = 'Backlog'
              AND NOT EXISTS (
                  SELECT 1
                  FROM issues i
                  WHERE i.project_id = projects.id
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM wiki_nodes w
                  WHERE w.project_id = projects.id
              )
              AND NOT EXISTS (
                  SELECT 1
                  FROM wiki_audits a
                  WHERE a.project_id = projects.id
              )
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
