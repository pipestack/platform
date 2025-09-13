use axum::{Json, http::StatusCode};
use sqlx::PgPool;

use crate::{DeployRequest, DeployResponse, config::AppConfig, config_converter, database};

pub async fn deploy_pipeline_to_wasm_cloud(
    payload: &DeployRequest,
    app_config: &AppConfig,
    db_pool: &PgPool,
) -> (StatusCode, Json<DeployResponse>) {
    // Convert payload to a valid wadm file
    let wadm_config = match config_converter::convert_pipeline(
        &payload.pipeline,
        &payload.workspace_slug,
        app_config,
    ) {
        Ok(config) => {
            tracing::info!("Successfully converted pipeline to WADM config");
            config
        }
        Err(e) => {
            tracing::error!("Failed to convert pipeline: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    result: format!("Error converting pipeline: {e}"),
                }),
            );
        }
    };

    // Convert to YAML string
    let wadm_yaml = match serde_yaml::to_string(&wadm_config) {
        Ok(yaml) => yaml,
        Err(e) => {
            tracing::error!("Failed to serialize WADM config to YAML: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    result: format!("Error serializing WADM config: {e}"),
                }),
            );
        }
    };

    tracing::info!("WADM yaml generated successfully: {wadm_yaml}");

    let nats_account = match get_nats_account(&payload.workspace_slug, db_pool).await {
        Ok(value) => value,
        Err(value) => return value,
    };

    let wadm_subject = format!("{}.wadm.api", nats_account);
    let client = match wadm_client::Client::new(
        &payload.workspace_slug,
        Some(&wadm_subject),
        wadm_client::ClientConnectOptions {
            ca_path: None,
            creds_path: None,
            jwt: app_config.nats.jwt.clone(),
            seed: app_config.nats.nkey.clone(),
            url: Some(app_config.nats.cluster_uris.clone()),
        },
    )
    .await
    {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("Failed to create WADM client: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    result: format!("Error creating WADM client: {e}"),
                }),
            );
        }
    };

    tracing::info!(
        "Putting and deploying manifest: {}",
        &wadm_config.metadata.name
    );
    client
        .put_and_deploy_manifest(wadm_yaml.as_bytes())
        .await
        .unwrap();

    (
        StatusCode::OK,
        Json(DeployResponse {
            result: "Pipeline deployed successfully".to_string(),
        }),
    )
}

pub async fn deploy_providers_to_wasm_cloud(
    workspace_slug: &str,
    app_config: &AppConfig,
    db_pool: &PgPool,
) -> (StatusCode, Json<DeployResponse>) {
    // Create providers wadm config
    let wadm_config = config_converter::create_providers_wadm(workspace_slug, app_config);

    // Convert to YAML string
    let wadm_yaml = match serde_yaml::to_string(&wadm_config) {
        Ok(yaml) => yaml,
        Err(e) => {
            tracing::error!("Failed to serialize providers WADM config to YAML: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    result: format!("Error serializing providers WADM config: {e}"),
                }),
            );
        }
    };

    tracing::info!("Providers WADM yaml generated successfully: {wadm_yaml}");

    let nats_account = match get_nats_account(workspace_slug, db_pool).await {
        Ok(value) => value,
        Err(value) => return value,
    };

    let wadm_subject = format!("{}.wadm.api", nats_account);
    let client = match wadm_client::Client::new(
        workspace_slug,
        Some(&wadm_subject),
        wadm_client::ClientConnectOptions {
            ca_path: None,
            creds_path: None,
            jwt: app_config.nats.jwt.clone(),
            seed: app_config.nats.nkey.clone(),
            url: Some(app_config.nats.cluster_uris.clone()),
        },
    )
    .await
    {
        Ok(client) => client,
        Err(e) => {
            tracing::error!("Failed to create WADM client: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    result: format!("Error creating WADM client: {e}"),
                }),
            );
        }
    };

    tracing::info!(
        "Putting and deploying providers manifest: {}",
        &wadm_config.metadata.name
    );

    match client.put_and_deploy_manifest(wadm_yaml.as_bytes()).await {
        Ok(_) => {
            tracing::info!("Providers deployed successfully");
            (
                StatusCode::OK,
                Json(DeployResponse {
                    result: "Providers deployed successfully".to_string(),
                }),
            )
        }
        Err(e) => {
            tracing::error!("Failed to deploy providers: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    result: format!("Error deploying providers: {e}"),
                }),
            )
        }
    }
}

async fn get_nats_account(
    workspace_slug: &str,
    db_pool: &sqlx::Pool<sqlx::Postgres>,
) -> Result<String, (StatusCode, Json<DeployResponse>)> {
    let nats_account = match database::get_workspace_nats_account(db_pool, workspace_slug).await {
        Ok(Some(account)) => account,
        Ok(None) => {
            tracing::error!("No NATS account found for workspace: {}", workspace_slug);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    result: format!(
                        "No NATS account configured for workspace: {}",
                        workspace_slug
                    ),
                }),
            ));
        }
        Err(e) => {
            tracing::error!(
                "Failed to fetch NATS account for workspace {}: {}",
                workspace_slug,
                e
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    result: format!("Error fetching workspace NATS account: {e}"),
                }),
            ));
        }
    };
    Ok(nats_account)
}
