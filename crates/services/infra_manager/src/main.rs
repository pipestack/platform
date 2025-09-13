mod config;
mod database;
mod infisical;
mod nats;
mod railway;

use anyhow::{Context, Result};
use config::AppConfig;
use infisical::InfisicalClient;
use nats::NatsManager;
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
    nats_manager: NatsManager,
    infisical_client: InfisicalClient,
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

        info!("Connecting to NATS at: {}", app_config.nats.url);
        let key_pair = std::sync::Arc::new(
            nkeys::KeyPair::from_seed(app_config.nats.nkey.clone().unwrap().as_str()).unwrap(),
        );
        let nats_client = async_nats::ConnectOptions::with_jwt(
            app_config.nats.jwt.clone().unwrap(),
            move |nonce| {
                let key_pair = key_pair.clone();
                async move { key_pair.sign(&nonce).map_err(async_nats::AuthError::new) }
            },
        )
        .connect(&app_config.nats.url)
        .await
        .context("Failed to connect to NATS")?;

        info!("Connecting to NATS as SYS user at: {}", app_config.nats.url);
        let key_pair_sys = std::sync::Arc::new(
            nkeys::KeyPair::from_seed(app_config.nats.sys_nkey.clone().unwrap().as_str()).unwrap(),
        );
        let nats_client_sys = async_nats::ConnectOptions::with_jwt(
            app_config.nats.sys_jwt.clone().unwrap(),
            move |nonce| {
                let key_pair_sys = key_pair_sys.clone();
                async move {
                    key_pair_sys
                        .sign(&nonce)
                        .map_err(async_nats::AuthError::new)
                }
            },
        )
        .connect(&app_config.nats.url)
        .await
        .context("Failed to connect to NATS as SYS user")?;

        info!("Initializing NATS manager...");
        let nats_manager = NatsManager::new(
            app_config.nats.operator_seed.clone(),
            app_config.nats.pipestack_account_seed.clone(),
            nats_client,
            nats_client_sys,
        )?;

        info!("Initializing Infisical client...");
        let infisical_client = InfisicalClient::new(app_config.infisical.clone()).await?;

        Ok(Self {
            app_config,
            pool,
            nats_manager,
            infisical_client,
        })
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

                    // Create NATS account and credentials for the workspace
                    let nats_credentials = match self
                        .nats_manager
                        .create_workspace_credentials(&workspace.slug, &self.pool)
                        .await
                    {
                        Ok(credentials) => {
                            info!("Created NATS credentials for workspace: {}", workspace.slug);

                            // Store credentials in Infisical
                            if let Err(e) = self
                                .infisical_client
                                .store_nats_credentials(&workspace.slug, &credentials)
                                .await
                            {
                                error!(
                                    "Failed to store NATS credentials in Infisical for workspace {}: {}",
                                    workspace.slug, e
                                );
                            } else {
                                info!(
                                    "NATS credentials stored in Infisical for workspace: {}",
                                    workspace.slug
                                );
                            }

                            info!(
                                "NATS credentials processing completed for workspace: {}",
                                workspace.slug
                            );
                            Some(credentials)
                        }
                        Err(e) => {
                            error!(
                                "Failed to create NATS credentials for workspace {}: {}",
                                workspace.slug, e
                            );
                            None
                        }
                    };

                    if let Some(credentials) = nats_credentials {
                        railway::try_to_create_service(&self.app_config, workspace, &credentials)
                            .await;
                    }
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

    if let Err(e) = infra_manager.infisical_client.test_connection().await {
        error!("Failed to connect to Infisical: {}", e);
        return Err(e);
    }

    info!("Infrastructure Manager service started successfully");
    infra_manager.listen_for_notifications().await?;

    Ok(())
}
