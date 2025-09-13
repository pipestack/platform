use anyhow::Result;
use sqlx::{Error, PgPool};
use tracing::{error, info};

pub async fn test_connection(pool: &PgPool) -> Result<(), Error> {
    let query = r#"
        SELECT count(*)
        FROM workspaces
    "#;

    match sqlx::query_scalar::<_, i64>(query).fetch_one(pool).await {
        Ok(count) => {
            info!("Total workspaces count: {}", count);
        }
        Err(e) => {
            error!("Failed to query workspace count: {}", e);
            return Err(e);
        }
    }

    info!("Database connection established successfully");
    Ok(())
}

pub async fn verify_workspaces_table(pool: &PgPool) -> Result<()> {
    info!("Verifying workspaces table exists...");

    let check_table_query = r#"
            SELECT EXISTS (
                SELECT FROM information_schema.tables 
                WHERE table_schema = 'public' 
                AND table_name = 'workspaces'
            );
        "#;

    let exists: bool = sqlx::query_scalar(check_table_query)
        .fetch_one(pool)
        .await?;

    if !exists {
        return Err(anyhow::anyhow!(
            "Workspaces table does not exist. This table should be created by another service."
        ));
    }

    info!("Workspaces table verified successfully");
    Ok(())
}

pub async fn setup_database_trigger(pool: &PgPool) -> Result<()> {
    info!("Setting up database trigger...");

    let trigger_function = r#"
            CREATE OR REPLACE FUNCTION notify_workspace_created()
            RETURNS TRIGGER AS $$
            BEGIN
                PERFORM pg_notify('workspace_created', 
                    json_build_object(
                        'slug', NEW.slug
                    )::text
                );
                RETURN NEW;
            END;
            $$ LANGUAGE plpgsql;
        "#;

    sqlx::query(trigger_function).execute(pool).await?;

    let drop_trigger_sql = r#"
            DROP TRIGGER IF EXISTS workspace_insert_trigger ON workspaces;
        "#;

    sqlx::query(drop_trigger_sql).execute(pool).await?;

    let create_trigger_sql = r#"
            CREATE TRIGGER workspace_insert_trigger
            AFTER INSERT ON workspaces
            FOR EACH ROW
            EXECUTE FUNCTION notify_workspace_created();
        "#;

    sqlx::query(create_trigger_sql).execute(pool).await?;

    info!("Database trigger setup completed successfully");
    Ok(())
}

pub async fn update_workspace_nats_account(
    pool: &PgPool,
    workspace_slug: &str,
    nats_account_public_key: &str,
) -> Result<()> {
    info!(
        "Updating workspace '{}' with NATS account public key: {}",
        workspace_slug, nats_account_public_key
    );

    let update_query = r#"
        UPDATE workspaces
        SET nats_account = $1
        WHERE slug = $2
    "#;

    let result = sqlx::query(update_query)
        .bind(nats_account_public_key)
        .bind(workspace_slug)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(anyhow::anyhow!(
            "No workspace found with slug: {}",
            workspace_slug
        ));
    }

    info!(
        "Successfully updated workspace '{}' with NATS account public key",
        workspace_slug
    );
    Ok(())
}
