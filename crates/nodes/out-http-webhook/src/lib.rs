use crate::exports::pipestack::out::out::Guest;
use crate::wasi::http::types::Fields;
use shared::{FromConfig, OutHttpWebhookSettings};
use wasmcloud_component::{error, info};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn run(input: String) -> String {
        let config = match wasi::config::runtime::get("json") {
            Ok(config) => config,
            Err(e) => {
                error!("Failed to get config: {e:?}");
                return format!("Failed to get config: {e:?}");
            }
        };

        let settings = match OutHttpWebhookSettings::from_config(config) {
            Ok(settings) => settings,
            Err(e) => {
                error!("Failed to parse config: {e}");
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
                    error!("Failed to set header {}: {}", header.key, e);
                });
        }
    }

    // Handle authentication
    if let Some(auth) = &settings.authentication {
        match auth.auth_type.as_str() {
            "api_key" => {
                if auth.config.location == "header" {
                    let auth_value = if auth.config.prefix.is_empty() {
                        auth.config.value.clone()
                    } else {
                        format!("{} {}", auth.config.prefix, auth.config.value)
                    };
                    fields
                        .set(&auth.config.name, &[auth_value.as_bytes().to_vec()])
                        .unwrap_or_else(|e| {
                            error!("Failed to set auth header {}: {}", auth.config.name, e);
                        });
                }
                // Query string auth will be handled when constructing the URL
            }
            "bearer" => {
                let bearer_value = format!("Bearer {}", auth.config.value);
                fields
                    .set("Authorization", &[bearer_value.as_bytes().to_vec()])
                    .unwrap_or_else(|e| {
                        error!("Failed to set Bearer authorization header: {}", e);
                    });
            }
            "basic" => {
                let basic_value = format!("Basic {}", auth.config.value);
                fields
                    .set("Authorization", &[basic_value.as_bytes().to_vec()])
                    .unwrap_or_else(|e| {
                        error!("Failed to set Basic authorization header: {}", e);
                    });
            }
            _ => {
                error!("Unsupported authentication type: {}", auth.auth_type);
            }
        }
    }

    let method = match settings.method.as_str() {
        "GET" => wasi::http::types::Method::Get,
        "POST" => wasi::http::types::Method::Post,
        "PUT" => wasi::http::types::Method::Put,
        "PATCH" => wasi::http::types::Method::Patch,
        "DELETE" => wasi::http::types::Method::Delete,
        "HEAD" => wasi::http::types::Method::Head,
        "OPTIONS" => wasi::http::types::Method::Options,
        _ => wasi::http::types::Method::Get,
    };

    // Parse the URL to extract scheme and authority
    let url_parts: Vec<&str> = settings.url.splitn(2, "://").collect();
    let (scheme, authority, mut path_with_query) = if url_parts.len() == 2 {
        let scheme = match url_parts[0] {
            "https" => wasi::http::types::Scheme::Https,
            "http" => wasi::http::types::Scheme::Http,
            _ => wasi::http::types::Scheme::Https,
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
            wasi::http::types::Scheme::Https,
            settings.url.as_str(),
            "/".to_string(),
        )
    };

    // Handle API key authentication in query string
    if let Some(auth) = &settings.authentication {
        if auth.auth_type == "api_key" && auth.config.location == "query" {
            let separator = if path_with_query.contains('?') {
                "&"
            } else {
                "?"
            };
            path_with_query = format!(
                "{}{}{}={}",
                path_with_query, separator, auth.config.name, auth.config.value
            );
        }
    }

    // Set Content-Type header for methods that will have a body
    if matches!(
        method,
        wasi::http::types::Method::Post
            | wasi::http::types::Method::Put
            | wasi::http::types::Method::Patch
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

    let req = wasi::http::outgoing_handler::OutgoingRequest::new(fields);
    req.set_method(&method).unwrap();
    req.set_scheme(Some(&scheme)).unwrap();
    req.set_authority(Some(authority)).unwrap();
    req.set_path_with_query(Some(path_with_query.as_str()))
        .unwrap();

    // Add request body for methods that support it
    if matches!(
        method,
        wasi::http::types::Method::Post
            | wasi::http::types::Method::Put
            | wasi::http::types::Method::Patch
    ) {
        let body = req.body().unwrap();
        let output_stream = body.write().unwrap();

        // Create JSON payload with the input
        let payload = format!(r#"{{"data": "{}"}}"#, input.replace('"', r#"\""#));
        output_stream
            .blocking_write_and_flush(payload.as_bytes())
            .unwrap_or_else(|e| {
                error!("Failed to write request body: {}", e);
            });

        drop(output_stream);
    }

    // Perform the HTTP request
    let _dog_picture_url = match wasi::http::outgoing_handler::handle(req, None) {
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
                loop {
                    match input_stream.read(1024) {
                        Ok(chunk) => {
                            if chunk.is_empty() {
                                break;
                            }
                            body_content.extend_from_slice(&chunk);
                        }
                        Err(_) => break,
                    }
                }

                let body_string = String::from_utf8_lossy(&body_content);
                info!(
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

export!(Component);
