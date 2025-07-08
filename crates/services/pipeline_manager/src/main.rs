use std::net::SocketAddr;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

mod config_converter;
mod registry;
mod settings;
mod wadm;

use crate::config_converter::Pipeline;
use settings::Settings;

#[derive(Clone)]
struct AppState {
    settings: Settings,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let settings = Settings::new().expect("Could not read config settings");
    let state = AppState { settings };

    let app = Router::new()
        .route("/deploy", post(deploy))
        .route("/health", get(health))
        .with_state(state);

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

async fn deploy(
    State(app_state): State<AppState>,
    Json(payload): Json<DeployRequest>,
) -> (StatusCode, Json<DeployResponse>) {
    tracing::info!("Received deploy request: {:?}", payload);

    if let Err(e) = crate::registry::publish_wasm_components(&payload, &app_state.settings).await {
        tracing::error!("Failed to publish WASM components: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(DeployResponse {
                result: format!("Failed to publish WASM components: {}", e),
            }),
        );
    }

    crate::wadm::deploy_to_wasm_cloud(&payload, &app_state.settings).await
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
