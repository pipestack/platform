use std::net::SocketAddr;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use shared::Pipeline;
use tokio::net::TcpListener;

use crate::config::AppConfig;

mod builders;
mod config;
mod config_converter;
mod database;
mod registry;
mod wadm;

#[derive(Clone)]
struct AppState {
    app_config: AppConfig,
    db_pool: sqlx::PgPool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app_config = AppConfig::new().expect("Could not read app config");

    tracing::info!("Connecting to database...");
    let db_pool = sqlx::PgPool::connect(&app_config.database.url)
        .await
        .expect("Failed to connect to database");

    if let Err(e) = database::test_connection(&db_pool).await {
        tracing::error!("Database connection test failed: {}", e);
        panic!("Failed to establish database connection");
    }

    let state = AppState {
        app_config,
        db_pool,
    };

    let app = Router::new()
        .route("/deploy", post(deploy_pipeline))
        .route("/deploy-providers", post(deploy_providers))
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

async fn deploy_pipeline(
    State(app_state): State<AppState>,
    Json(payload): Json<DeployRequest>,
) -> (StatusCode, Json<DeployResponse>) {
    tracing::info!("Received deploy request: {:?}", payload);

    if let Err(e) = crate::registry::publish_wasm_components(&payload, &app_state.app_config).await
    {
        tracing::error!("Failed to publish WASM components: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(DeployResponse {
                result: format!("Failed to publish WASM components: {e}"),
            }),
        );
    }

    crate::wadm::deploy_pipeline_to_wasm_cloud(&payload, &app_state.app_config, &app_state.db_pool)
        .await
}

async fn deploy_providers(
    State(app_state): State<AppState>,
    Json(payload): Json<DeployProvidersRequest>,
) -> (StatusCode, Json<DeployResponse>) {
    tracing::info!("Received deploy-providers request: {:?}", payload);

    crate::wadm::deploy_providers_to_wasm_cloud(
        &payload.workspace_slug,
        &app_state.app_config,
        &app_state.db_pool,
    )
    .await
}

#[derive(Debug, Deserialize, Serialize)]
struct DeployRequest {
    pipeline: Pipeline,
    #[serde(rename = "workspaceSlug")]
    workspace_slug: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct DeployProvidersRequest {
    #[serde(rename = "workspaceSlug")]
    workspace_slug: String,
}

#[derive(Deserialize, Serialize)]
struct DeployResponse {
    result: String,
}
