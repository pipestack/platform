use shared::{FromConfig, InHttpWebhookSettings};
use wasmcloud_component::{
    error,
    http::{self, ErrorCode, Response, StatusCode},
};

mod bindings {
    use wasmcloud_component::http;

    use super::Component;
    wit_bindgen::generate!({ generate_all });
    http::export!(Component);
}

struct Component;

const LOG_CONTEXT: &str = "in-http";

impl http::Server for Component {
    fn handle(
        request: http::IncomingRequest,
    ) -> http::Result<http::Response<impl http::OutgoingBody>> {
        let config = match bindings::wasi::config::runtime::get("json") {
            Ok(config) => config,
            Err(e) => {
                error!(context: LOG_CONTEXT, "Failed to get config: {e:?}");
                return Ok(http::Response::new("Internal server error\n".to_string()));
            }
        };

        let settings = match InHttpWebhookSettings::from_config(config) {
            Ok(settings) => settings,
            Err(e) => {
                error!(context: LOG_CONTEXT, "Failed to parse config: {e:?}");
                return Ok(http::Response::new("Invalid configuration\n".to_string()));
            }
        };

        if request.method().to_string() != settings.method {
            error!(context: LOG_CONTEXT, "Method mismatch: expected {:?}, got {:?}",
            settings.method,
            request.method());
            return Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .body("Method not allowed".to_string())
                .map_err(|e| {
                    ErrorCode::InternalError(Some(format!("failed to build response: {e:?}")))
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
        let received = bindings::pipestack::out::out::run(message);
        Ok(http::Response::new(format!("{received}\n")))
    }
}
