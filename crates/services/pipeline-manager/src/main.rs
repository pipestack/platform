use axum::{Json, Router, http::StatusCode, routing::post};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cors = CorsLayer::new()
        .allow_origin(
            "http://localhost:5173"
                .parse::<axum::http::HeaderValue>()
                .unwrap(),
        )
        .allow_methods([axum::http::Method::POST])
        .allow_headers(Any);

    let app = Router::new().route("/deploy", post(deploy)).layer(cors);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001").await.unwrap();
    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn deploy(Json(payload): Json<DeployRequest>) -> (StatusCode, Json<DeployResponse>) {
    tracing::info!("Received deploy request: {:?}", payload);

    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let work_dir = format!("{}/projects/github/pipestack/platform/main", home_dir);

    tracing::info!("Building and deploying pipeline: {work_dir}");

    match Command::new("just")
        .args(&["wash-deploy-example", "02"])
        .current_dir(&work_dir)
        .output()
        .await
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                tracing::info!("Command executed successfully: {}", stdout);
                (
                    StatusCode::OK,
                    Json(DeployResponse {
                        result: format!("Deploy completed successfully: {}", stdout),
                    }),
                )
            } else {
                tracing::error!("Command failed with stderr: {}", stderr);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(DeployResponse {
                        result: format!("Deploy failed: {}", stderr),
                    }),
                )
            }
        }
        Err(e) => {
            tracing::error!("Failed to execute command: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(DeployResponse {
                    result: format!("Failed to execute deploy command: {}", e),
                }),
            )
        }
    }

    // match call_component_composer_service(&payload).await {
    //     Ok(response) => (StatusCode::OK, Json(response)),
    //     Err(e) => {
    //         tracing::error!("Deploy failed: {}", e);
    //         (
    //             StatusCode::INTERNAL_SERVER_ERROR,
    //             Json(DeployResponse {
    //                 result: format!("Deploy failed: {}", e),
    //             }),
    //         )
    //     }
    // }
}

// async fn call_component_composer_service(
//     payload: &DeployRequest,
// ) -> Result<DeployResponse, String> {
//     let client = reqwest::Client::new();
//     let response = client
//         .post("http://localhost:3000/compose")
//         .json(payload)
//         .send()
//         .await
//         .map_err(|e| format!("Request failed: {}", e))?;

//     tracing::info!("POST request successful: {}", response.status());

//     response
//         .json::<DeployResponse>()
//         .await
//         .map_err(|e| format!("Failed to parse response: {}", e))
// }

#[derive(Debug, Deserialize, Serialize)]
struct DeployRequest {
    pipeline: serde_json::Value,
}

#[derive(Deserialize, Serialize)]
struct DeployResponse {
    result: String,
}
