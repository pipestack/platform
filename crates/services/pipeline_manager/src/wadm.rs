use axum::{Json, http::StatusCode};

use crate::{DeployRequest, DeployResponse, config_converter, settings::Settings};

pub async fn deploy_pipeline_to_wasm_cloud(
    payload: &DeployRequest,
    settings: &Settings,
) -> (StatusCode, Json<DeployResponse>) {
    // Convert payload to a valid wadm file
    let wadm_config = match config_converter::convert_pipeline(
        &payload.pipeline,
        &payload.workspace_slug,
        settings,
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
                    result: format!("Error converting pipeline: {}", e),
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
                    result: format!("Error serializing WADM config: {}", e),
                }),
            );
        }
    };

    tracing::info!("WADM yaml generated successfully: {wadm_yaml}");

    let client = match wadm_client::Client::new(
        &payload.workspace_slug,
        None,
        wadm_client::ClientConnectOptions {
            ca_path: None,
            creds_path: None,
            jwt: None,
            seed: None,
            url: Some(settings.nats.cluster_uris.clone()),
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
                    result: format!("Error creating WADM client: {}", e),
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
    settings: &Settings,
) -> (StatusCode, Json<DeployResponse>) {
    // Create providers wadm config
    let wadm_config = config_converter::create_providers_wadm(workspace_slug, settings);

    // Convert to YAML string
    let wadm_yaml = match serde_yaml::to_string(&wadm_config) {
        Ok(yaml) => yaml,
        Err(e) => {
            tracing::error!("Failed to serialize providers WADM config to YAML: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    result: format!("Error serializing providers WADM config: {}", e),
                }),
            );
        }
    };

    tracing::info!("Providers WADM yaml generated successfully: {wadm_yaml}");

    let client = match wadm_client::Client::new(
        workspace_slug,
        None,
        wadm_client::ClientConnectOptions {
            ca_path: None,
            creds_path: None,
            jwt: None,
            seed: None,
            url: Some(settings.nats.cluster_uris.clone()),
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
                    result: format!("Error creating WADM client: {}", e),
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
                    result: format!("Error deploying providers: {}", e),
                }),
            )
        }
    }
}
