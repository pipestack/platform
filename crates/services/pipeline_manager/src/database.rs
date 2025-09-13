use anyhow::Result;
use sqlx::PgPool;
use tracing::{error, info};

pub async fn get_workspace_nats_account(
    pool: &PgPool,
    workspace_slug: &str,
) -> Result<Option<String>> {
    info!("Fetching NATS account for workspace: {}", workspace_slug);

    let query = r#"
        SELECT nats_account
        FROM workspaces
        WHERE slug = $1
    "#;

    match sqlx::query_scalar::<_, Option<String>>(query)
        .bind(workspace_slug)
        .fetch_one(pool)
        .await
    {
        Ok(nats_account) => {
            if let Some(ref account) = nats_account {
                info!(
                    "Found NATS account for workspace '{}': {}",
                    workspace_slug, account
                );
            } else {
                info!("No NATS account found for workspace '{}'", workspace_slug);
            }
            Ok(nats_account)
        }
        Err(sqlx::Error::RowNotFound) => {
            error!("Workspace '{}' not found in database", workspace_slug);
            Err(anyhow::anyhow!("Workspace '{}' not found", workspace_slug))
        }
        Err(e) => {
            error!(
                "Database error while fetching NATS account for workspace '{}': {}",
                workspace_slug, e
            );
            Err(anyhow::anyhow!("Database error: {}", e))
        }
    }
}

pub async fn test_connection(pool: &PgPool) -> Result<(), sqlx::Error> {
    let query = r#"
        SELECT count(*)
        FROM workspaces
    "#;

    match sqlx::query_scalar::<_, i64>(query).fetch_one(pool).await {
        Ok(count) => {
            info!(
                "Database connection test successful. Total workspaces count: {}",
                count
            );
            Ok(())
        }
        Err(e) => {
            error!("Failed to test database connection: {}", e);
            Err(e)
        }
    }
}
