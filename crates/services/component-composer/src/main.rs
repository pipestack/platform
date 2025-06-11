use axum::{Json, Router, http::StatusCode, routing::post};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new().route("/compose", post(compose));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn compose(Json(payload): Json<ComposeRequest>) -> (StatusCode, Json<ComposeResponse>) {
    tracing::info!("Received compose request: {:?}", payload);
    let wac_version = match Command::new("wac").arg("--version").output().await {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        Err(e) => format!("Error executing wac: {}", e),
    };

    let response = ComposeResponse {
        id: 1337,
        result: format!(
            "Pipeline config: {}, WAC Version: {}",
            payload.pipeline, wac_version
        ),
    };

    (StatusCode::CREATED, Json(response))
}

#[derive(Debug, Deserialize)]
struct ComposeRequest {
    pipeline: serde_json::Value,
}

#[derive(Serialize)]
struct ComposeResponse {
    id: u64,
    result: String,
}
