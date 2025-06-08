use axum::{
    routing::{get, post},
    http::StatusCode,
    Json, Router,
};
use serde::{Deserialize, Serialize};
    use std::process::Command;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(root))
        .route("/compose", post(compose));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    "Hello, World!"
}

async fn compose(
    Json(payload): Json<ComposeRequest>,
) -> (StatusCode, Json<ComposeResponse>) {
    let wac_version = match Command::new("wac").arg("--version").output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        Err(e) => format!("Error executing wac: {}", e),
    };

    let response = ComposeResponse {
        id: 1337,
        result: format!("Composed: {}, WAC Version: {}", payload.input, wac_version),
    };

    (StatusCode::CREATED, Json(response))
}

#[derive(Deserialize)]
struct ComposeRequest {
    input: String,
}

#[derive(Serialize)]
struct ComposeResponse {
    id: u64,
    result: String,
}