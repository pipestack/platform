wit_bindgen::generate!({ generate_all });

use shared::{FromConfig, InHttpWebhookSettings};
use wasmcloud_component::{
    http::{self, ErrorCode, Response, StatusCode},
    wasi::logging::logging::{log, Level},
};

struct Component;

http::export!(Component);

impl http::Server for Component {
    fn handle(
        request: http::IncomingRequest,
    ) -> http::Result<http::Response<impl http::OutgoingBody>> {
        let config = match wasi::config::runtime::get_all() {
            Ok(config) => config,
            Err(e) => {
                log(
                    Level::Error,
                    "in-http",
                    &format!("Failed to get config: {e:?}"),
                );
                return Ok(http::Response::new("Internal server error\n".to_string()));
            }
        };

        let settings = match InHttpWebhookSettings::from_config(config) {
            Ok(settings) => settings,
            Err(e) => {
                log(
                    Level::Error,
                    "in-http",
                    &format!("Failed to parse config: {e}"),
                );
                return Ok(http::Response::new("Invalid configuration\n".to_string()));
            }
        };

        if request.method().to_string() != settings.method {
            log(
                Level::Error,
                "in-http",
                &format!(
                    "Method mismatch: expected {:?}, got {:?}",
                    settings.method,
                    request.method()
                ),
            );
            return Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .body("Method not allowed".to_string())
                .map_err(|e| {
                    ErrorCode::InternalError(Some(format!("failed to build response {e:?}")))
                });
        }

        let message = request
            .uri()
            .query()
            .and_then(|query| {
                query.split('&').find_map(|param| {
                    let mut parts = param.split('=');
                    match (parts.next(), parts.next()) {
                        (Some("message"), Some(value)) => Some(value),
                        _ => None,
                    }
                })
            })
            .unwrap_or("default message");
        let received = pipestack::out::out::run(message);
        Ok(http::Response::new(format!("{received}\n")))
    }
}
