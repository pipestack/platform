use std::net::SocketAddr;

use axum::{
    Json, Router,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

mod config_converter;

use crate::config_converter::Pipeline;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/deploy", post(deploy))
        .route("/health", get(health));

    let port: u16 = std::env::var("PORT")
        .unwrap_or("3000".into())
        .parse()
        .expect("failed to convert PORT env var to number");
    let ipv6 = SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 0], port));
    let ipv6_listener = TcpListener::bind(&ipv6).await.unwrap();

    tracing::info!("listening on {}", ipv6_listener.local_addr().unwrap());
    axum::serve(ipv6_listener, app).await.unwrap();
}

async fn health() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "healthy"
        })),
    )
}

async fn deploy(Json(payload): Json<DeployRequest>) -> (StatusCode, Json<DeployResponse>) {
    tracing::info!("Received deploy request: {:?}", payload);

    // Convert payload to a valid wadm file
    let wadm_config = match config_converter::convert_pipeline(&payload.pipeline, &payload.workspace_slug) {
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

    // TODO: Deploy wadm file
    let client = wadm_client::Client::new(
        "default",
        None,
        wadm_client::ClientConnectOptions {
            ca_path: None,
            creds_path: None,
            jwt: None,
            seed: None,
            url: std::option_env!("NATS_URL").map(String::from),
        },
    )
    .await
    .unwrap();

    // TODO: Only undeploy and delete the manifest in DEV, not PROD
    tracing::info!("Undeploying manifest: {}", &wadm_config.metadata.name);
    let _ = client.undeploy_manifest(&wadm_config.metadata.name).await;
    tracing::info!("Deleting manifest: {}", &wadm_config.metadata.name);
    client
        .delete_manifest(&wadm_config.metadata.name, None)
        .await
        .unwrap();
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
            result: format!("Pipeline deployed successfully"),
        }),
    )
}

#[derive(Debug, Deserialize, Serialize)]
struct DeployRequest {
    pipeline: Pipeline,
    #[serde(rename = "workspaceSlug")]
    workspace_slug: String,
}

#[derive(Deserialize, Serialize)]
struct DeployResponse {
    result: String,
}
