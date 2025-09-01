mod config;
mod database;
mod railway;

use anyhow::Result;
use config::Config;
use serde::Deserialize;
use sqlx::{PgPool, postgres::PgListener};
use tracing::{error, info};

#[derive(Debug, Deserialize)]
struct WorkspaceNotification {
    slug: String,
}

struct InfraManager {
    config: Config,
    pool: PgPool,
}

impl InfraManager {
    async fn new() -> Result<Self> {
        let config = Config::from_env()?;
        config.validate()?;

        info!("Connecting to database...");
        let pool = PgPool::connect(&config.database.url).await?;
        database::test_connection(&pool).await?;

        Ok(Self { config, pool })
    }

    async fn listen_for_notifications(&self) -> Result<()> {
        info!("Starting notification listener...");

        let mut listener = PgListener::connect(&self.config.database.url).await?;
        listener
            .listen(&self.config.database.notification_channel)
            .await?;

        info!(
            "Started listening for workspace changes on channel: {}",
            self.config.database.notification_channel
        );

        loop {
            let notification = listener.recv().await?;
            info!("Received notification: {}", notification.payload());

            match serde_json::from_str::<WorkspaceNotification>(notification.payload()) {
                Ok(workspace) => {
                    info!("Processing new workspace: {:?}", workspace);

                    railway::try_to_create_service(&self.config, workspace).await;
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
