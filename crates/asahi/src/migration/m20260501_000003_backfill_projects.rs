use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        db.execute_unprepared(
            r#"
            INSERT INTO projects (
                id,
                slug,
                name,
                description,
                priority,
                state,
                url,
                created_at,
                updated_at
            )
            SELECT
                lower(hex(randomblob(16))),
                lower(replace(trim(i.project_slug), ' ', '-')),
                trim(i.project_slug),
                NULL,
                NULL,
                'Backlog',
                '/api/projects/' || lower(replace(trim(i.project_slug), ' ', '-')),
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP
            FROM issues i
            WHERE trim(i.project_slug) != ''
              AND lower(trim(i.project_slug)) != 'default'
              AND NOT EXISTS (
                  SELECT 1
                  FROM projects p
                  WHERE lower(p.slug) = lower(replace(trim(i.project_slug), ' ', '-'))
              )
            GROUP BY lower(replace(trim(i.project_slug), ' ', '-'))
            "#,
        )
        .await?;

        db.execute_unprepared(
            r#"
            UPDATE issues
            SET project_id = (
                SELECT p.id
                FROM projects p
                WHERE lower(p.slug) = lower(replace(trim(issues.project_slug), ' ', '-'))
                LIMIT 1
            )
            WHERE project_id IS NULL
              AND trim(project_slug) != ''
              AND lower(trim(project_slug)) != 'default'
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                UPDATE issues
                SET project_id = NULL
                WHERE project_id IN (
                    SELECT id
                    FROM projects
                    WHERE description IS NULL
                      AND priority IS NULL
                      AND state = 'Backlog'
                      AND created_at = updated_at
                )
                "#,
            )
            .await?;

        Ok(())
    }
}
