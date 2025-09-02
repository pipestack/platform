mod config;
mod database;
mod railway;

use anyhow::Result;
use config::AppConfig;
use serde::Deserialize;
use sqlx::{PgPool, postgres::PgListener};
use tracing::{error, info};

#[derive(Debug, Deserialize)]
struct WorkspaceNotification {
    slug: String,
}

struct InfraManager {
    app_config: AppConfig,
    pool: PgPool,
}

impl InfraManager {
    async fn new() -> Result<Self> {
        let app_config =
            AppConfig::new().map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
        app_config
            .validate()
            .map_err(|e| anyhow::anyhow!("Config validation failed: {}", e))?;

        info!("Connecting to database...");
        let pool = PgPool::connect(&app_config.database.url).await?;
        database::test_connection(&pool).await?;

        Ok(Self { app_config, pool })
    }

    async fn listen_for_notifications(&self) -> Result<()> {
        info!("Starting notification listener...");

        let mut listener = PgListener::connect(&self.app_config.database.url).await?;
        listener
            .listen(&self.app_config.database.notification_channel)
            .await?;

        info!(
            "Started listening for workspace changes on channel: {}",
            self.app_config.database.notification_channel
        );

        loop {
            let notification = listener.recv().await?;
            info!("Received notification: {}", notification.payload());

            match serde_json::from_str::<WorkspaceNotification>(notification.payload()) {
                Ok(workspace) => {
                    info!("Processing new workspace: {:?}", workspace);

                    railway::try_to_create_service(&self.app_config, workspace).await;
                }
                Err(e) => {
                    error!("Failed to parse notification payload: {}", e);
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting Infrastructure Manager service...");
    let infra_manager = InfraManager::new().await?;

    if let Err(e) = database::verify_workspaces_table(&infra_manager.pool).await {
        error!("Failed to verify workspaces table: {}", e);
        return Err(e);
    }

    if let Err(e) = database::setup_database_trigger(&infra_manager.pool).await {
        error!("Failed to setup database trigger: {}", e);
        return Err(e);
    }

    info!("Infrastructure Manager service started successfully");
    infra_manager.listen_for_notifications().await?;

    Ok(())
}
