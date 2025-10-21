use bindings::exports::pipestack::out::out::Guest;
use bindings::wasi::http::types::Fields;
use shared::{FromConfig, OutHttpWebhookSettings};
use wasmcloud_component::{error, info};

mod bindings {
    use super::Component;
    wit_bindgen::generate!({ generate_all });
    export!(Component);
}

struct Component;

const LOG_CONTEXT: &str = "out-http-webhook";

impl Guest for Component {
    fn run(input: String) -> String {
        let config = match bindings::wasi::config::runtime::get("json") {
            Ok(config) => config,
            Err(e) => {
                error!(context: LOG_CONTEXT, "Failed to get config: {e:?}");
                return format!("Failed to get config: {e:?}");
            }
        };

        let settings = match OutHttpWebhookSettings::from_config(config) {
            Ok(settings) => settings,
            Err(e) => {
                error!(context: LOG_CONTEXT, "Failed to parse config: {e}");
                return format!("Failed to parse config: {e}");
            }
        };

        match make_http_request(&input, &settings) {
            Ok(response) => response,
            Err(e) => format!("Error: {e}"),
        }
    }
}

fn make_http_request(input: &str, settings: &OutHttpWebhookSettings) -> Result<String, String> {
    // Create Fields with headers from settings
    let fields = Fields::new();
    if let Some(headers) = &settings.headers {
        for header in headers {
            fields
                .set(&header.key, &[header.value.as_bytes().to_vec()])
                .unwrap_or_else(|e| {
                    error!(context: LOG_CONTEXT, "Failed to set header {}: {}", header.key, e);
                });
        }
    }

    // Handle authentication
    if let Some(auth) = &settings.authentication {
        if let Some(config) = &auth.config {
            match auth.auth_type.as_str() {
                "api_key" => {
                    if config.location == "header" {
                        let auth_value = if config.prefix.is_empty() {
                            config.value.clone()
                        } else {
                            format!("{} {}", config.prefix, config.value)
                        };
                        fields
                            .set(&config.name, &[auth_value.as_bytes().to_vec()])
                            .unwrap_or_else(|e| {
                                error!(context: LOG_CONTEXT, "Failed to set auth header {}: {}", config.name, e);
                            });
                    }
                    // Query string auth will be handled when constructing the URL
                }
                "bearer" => {
                    let bearer_value = format!("Bearer {}", config.value);
                    fields
                        .set("Authorization", &[bearer_value.as_bytes().to_vec()])
                        .unwrap_or_else(|e| {
                            error!(context: LOG_CONTEXT, "Failed to set Bearer authorization header: {}", e);
                        });
                }
                "basic" => {
                    let basic_value = format!("Basic {}", config.value);
                    fields
                        .set("Authorization", &[basic_value.as_bytes().to_vec()])
                        .unwrap_or_else(|e| {
                            error!(context: LOG_CONTEXT, "Failed to set Basic authorization header: {}", e);
                        });
                }
                _ => {
                    error!(context: LOG_CONTEXT, "Unsupported authentication type: {}", auth.auth_type);
                }
            }
        } else {
            error!(context: LOG_CONTEXT, "Authentication config is missing for auth type: {}", auth.auth_type);
        }
    }

    let method = match settings.method.as_str() {
        "GET" => bindings::wasi::http::types::Method::Get,
        "POST" => bindings::wasi::http::types::Method::Post,
        "PUT" => bindings::wasi::http::types::Method::Put,
        "PATCH" => bindings::wasi::http::types::Method::Patch,
        "DELETE" => bindings::wasi::http::types::Method::Delete,
        "HEAD" => bindings::wasi::http::types::Method::Head,
        "OPTIONS" => bindings::wasi::http::types::Method::Options,
        _ => bindings::wasi::http::types::Method::Get,
    };

    // Parse the URL to extract scheme and authority
    let url_parts: Vec<&str> = settings.url.splitn(2, "://").collect();
    let (scheme, authority, mut path_with_query) = if url_parts.len() == 2 {
        let scheme = match url_parts[0] {
            "https" => bindings::wasi::http::types::Scheme::Https,
            "http" => bindings::wasi::http::types::Scheme::Http,
            _ => bindings::wasi::http::types::Scheme::Https,
        };
        let remaining = url_parts[1];
        let authority_and_path: Vec<&str> = remaining.splitn(2, '/').collect();
        let authority = authority_and_path[0];
        let path_with_query = if authority_and_path.len() == 2 {
            format!("/{}", authority_and_path[1])
        } else {
            "/".to_string()
        };
        (scheme, authority, path_with_query)
    } else {
        (
            bindings::wasi::http::types::Scheme::Https,
            settings.url.as_str(),
            "/".to_string(),
        )
    };

    // Handle API key authentication in query string
    if let Some(auth) = &settings.authentication
        && auth.auth_type == "api_key"
        && auth
            .config
            .as_ref()
            .is_some_and(|config| config.location == "query")
        && let Some(config) = &auth.config
    {
        let separator = if path_with_query.contains('?') {
            "&"
        } else {
            "?"
        };
        path_with_query = format!(
            "{}{}{}={}",
            path_with_query, separator, config.name, config.value
        );
    }

    // Set Content-Type header for methods that will have a body
    if matches!(
        method,
        bindings::wasi::http::types::Method::Post
            | bindings::wasi::http::types::Method::Put
            | bindings::wasi::http::types::Method::Patch
    ) {
        // Set Content-Type header from settings, defaulting to application/json
        let content_type = settings
            .content_type
            .as_deref()
            .unwrap_or("application/json");
        fields
            .set("Content-Type", &[content_type.as_bytes().to_vec()])
            .unwrap_or_else(|e| {
                error!("Failed to set Content-Type header: {}", e);
            });
    }

    let req = bindings::wasi::http::outgoing_handler::OutgoingRequest::new(fields);
    req.set_method(&method).unwrap();
    req.set_scheme(Some(&scheme)).unwrap();
    req.set_authority(Some(authority)).unwrap();
    req.set_path_with_query(Some(path_with_query.as_str()))
        .unwrap();

    // Add request body for methods that support it
    if matches!(
        method,
        bindings::wasi::http::types::Method::Post
            | bindings::wasi::http::types::Method::Put
            | bindings::wasi::http::types::Method::Patch
    ) {
        let body = req.body().unwrap();
        let output_stream = body.write().unwrap();

        // Create JSON payload with the input
        let payload = format!(r#"{{"data": "{}"}}"#, input.replace('"', r#"\""#));
        output_stream
            .blocking_write_and_flush(payload.as_bytes())
            .unwrap_or_else(|e| {
                error!(context: LOG_CONTEXT, "Failed to write request body: {}", e);
            });

        drop(output_stream);
    }

    // Perform the HTTP request
    let _ = match bindings::wasi::http::outgoing_handler::handle(req, None) {
        Ok(resp) => {
            resp.subscribe().block();
            let response = resp
                .get()
                .expect("HTTP request response missing")
                .expect("HTTP request response requested more than once")
                .expect("HTTP request failed");
            if response.status() == 200 {
                let response_body_stream = response
                    .consume()
                    .expect("failed to get incoming request body");
                let input_stream = response_body_stream
                    .stream()
                    .expect("failed to get response body stream");

                let mut body_content = Vec::new();
                while let Ok(chunk) = input_stream.read(1024) {
                    if chunk.is_empty() {
                        break;
                    }
                    body_content.extend_from_slice(&chunk);
                }

                let body_string = String::from_utf8_lossy(&body_content);
                info!(context: LOG_CONTEXT,
                    "Response status code: {}. Body: {}",
                    response.status(),
                    body_string
                );
                format!(
                    "HTTP request succeeded with status code {}",
                    response.status()
                )
            } else {
                format!("HTTP request failed with status code {}", response.status())
            }
        }
        Err(e) => {
            format!("Got error when trying to fetch dog: {e}")
        }
    };

    Ok("Done".into())
}
