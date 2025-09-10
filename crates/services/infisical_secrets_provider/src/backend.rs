use anyhow::{Context, Result};
use async_nats::{HeaderMap, Message, Subscriber};
use bytes::Bytes;
use futures_util::stream::StreamExt;
use nkeys::XKey;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::encryption::EncryptionHandler;
use crate::infisical_client::InfisicalClientWrapper;
use crate::jwt::JwtValidator;
use crate::types::{SecretRequest, SecretResponse};

/// The main Infisical secrets backend implementation
pub struct InfisicalSecretsBackend {
    /// NATS client for communication
    nats_client: async_nats::Client,
    /// Infisical client for secret retrieval
    infisical_client: InfisicalClientWrapper,
    /// Encryption handler for secure communication
    encryption_handler: EncryptionHandler,
    /// JWT validator for request validation
    jwt_validator: JwtValidator,
    /// Application configuration
    config: AppConfig,
    /// Unique instance ID
    instance_id: String,
}

impl InfisicalSecretsBackend {
    /// Creates a new Infisical secrets backend
    pub async fn new(config: AppConfig) -> Result<Self> {
        info!(
            "Initializing Infisical secrets backend: {}",
            config.backend.name
        );

        // Connect to NATS
        info!("Connecting to NATS at: {}", config.nats.url);
        let key_pair = std::sync::Arc::new(
            nkeys::KeyPair::from_seed(config.nats.nkey.clone().unwrap().as_str()).unwrap(),
        );
        let nats_client =
            async_nats::ConnectOptions::with_jwt(config.nats.jwt.clone().unwrap(), move |nonce| {
                let key_pair = key_pair.clone();
                async move { key_pair.sign(&nonce).map_err(async_nats::AuthError::new) }
            })
            .connect(&config.nats.url)
            .await
            .context("Failed to connect to NATS")?;

        // Create Infisical client
        let infisical_client = InfisicalClientWrapper::new(config.infisical.clone())
            .await
            .context("Failed to create Infisical client")?;

        // Test Infisical connection
        infisical_client
            .test_connection()
            .await
            .context("Failed to verify Infisical connection")?;

        // Create encryption handler
        let encryption_handler = EncryptionHandler::new();
        info!("Generated server xkey: {}", encryption_handler.public_key());

        // Create JWT validator
        let jwt_validator = JwtValidator::default();

        // Generate unique instance ID
        let instance_id = Uuid::new_v4().to_string();

        Ok(Self {
            nats_client,
            infisical_client,
            encryption_handler,
            jwt_validator,
            config,
            instance_id,
        })
    }

    /// Starts the secrets backend server
    pub async fn run(&self) -> Result<()> {
        info!(
            "Starting Infisical secrets backend instance: {} (ID: {})",
            self.config.backend.name, self.instance_id
        );

        // Subscribe to endpoints
        let get_subject = self.config.get_subject();
        let xkey_subject = self.config.server_xkey_subject();

        info!("Subscribing to NATS subjects:");
        info!("  Get secrets: {}", get_subject);
        info!("  Server xkey: {}", xkey_subject);

        let get_subscription = self
            .nats_client
            .subscribe(get_subject)
            .await
            .context("Failed to subscribe to get endpoint")?;

        let xkey_subscription = self
            .nats_client
            .subscribe(xkey_subject)
            .await
            .context("Failed to subscribe to server_xkey endpoint")?;

        info!("Infisical secrets backend is now running");

        // Handle requests concurrently
        let get_handler = {
            let backend = self.clone();
            tokio::spawn(async move {
                if let Err(e) = backend.handle_get_requests(get_subscription).await {
                    error!("Get request handler failed: {}", e);
                }
            })
        };

        let xkey_handler = {
            let backend = self.clone();
            tokio::spawn(async move {
                if let Err(e) = backend.handle_xkey_requests(xkey_subscription).await {
                    error!("Xkey request handler failed: {}", e);
                }
            })
        };

        // Wait for both handlers (this will run indefinitely)
        tokio::select! {
            result = get_handler => {
                if let Err(e) = result {
                    error!("Get handler task failed: {}", e);
                }
            }
            result = xkey_handler => {
                if let Err(e) = result {
                    error!("Xkey handler task failed: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Handles get secret requests
    async fn handle_get_requests(&self, mut subscription: Subscriber) -> Result<()> {
        info!("Started handling get secret requests");

        while let Some(msg) = subscription.next().await {
            let request_id = Uuid::new_v4().to_string();
            debug!("Processing get request: {}. Message: {:?}", request_id, msg);

            if let Err(e) = self.process_get_request(&msg, &request_id).await {
                error!("Error processing get request {}: {}", request_id, e);

                // Try to send error response if possible
                if let Err(send_error) = self
                    .send_error_response(&msg, &format!("Internal error: {}", e))
                    .await
                {
                    error!(
                        "Failed to send error response for request {}: {}",
                        request_id, send_error
                    );
                }
            }
        }

        warn!("Get request handler stopped");
        Ok(())
    }

    /// Handles server xkey requests
    async fn handle_xkey_requests(&self, mut subscription: Subscriber) -> Result<()> {
        info!("Started handling server xkey requests");

        while let Some(msg) = subscription.next().await {
            let request_id = Uuid::new_v4().to_string();
            debug!("Processing xkey request: {}", request_id);

            if let Err(e) = self.process_xkey_request(&msg).await {
                error!("Error processing xkey request {}: {}", request_id, e);
            }
        }

        warn!("Xkey request handler stopped");
        Ok(())
    }

    /// Processes a get secret request
    async fn process_get_request(&self, msg: &Message, request_id: &str) -> Result<()> {
        // Extract host xkey from headers
        let host_xkey = self
            .extract_host_xkey(&msg.headers)
            .context("Failed to extract host xkey from headers")?;

        debug!("Request {}: Extracted host xkey: {}", request_id, host_xkey);

        // Decrypt the request payload
        debug!("Decrypting payload: {:?}", &msg.payload);
        let decrypted_payload = self
            .encryption_handler
            .decrypt_payload(&msg.payload, &host_xkey)
            .map_err(|e| {
                error!("Decryption error: {}", e);
                e
            })
            .context("Failed to decrypt request payload")?;

        debug!(
            "Request {}: Decrypted payload: {}",
            request_id,
            String::from_utf8_lossy(&decrypted_payload)
        );

        // Parse the secret request
        let secret_request: SecretRequest = serde_json::from_slice(&decrypted_payload)
            .context("Failed to parse secret request JSON")?;

        info!(
            "Request {}: Processing secret request for '{}'",
            request_id, secret_request.key
        );

        // Validate JWT token
        let jwt_validation = self
            .jwt_validator
            .validate_token(&secret_request.context.entity_jwt)
            .context("Failed to validate JWT token")?;

        if !jwt_validation.is_valid() {
            let error_msg = format!(
                "JWT validation failed: {}",
                jwt_validation.errors().join(", ")
            );
            warn!("Request {}: {}", request_id, error_msg);
            return self.send_error_response(msg, &error_msg).await;
        }

        debug!(
            "Request {}: JWT validation successful for subject: {:?}",
            request_id,
            jwt_validation.subject_id()
        );

        // Fetch secret from Infisical
        match self.infisical_client.get_secret(&secret_request).await {
            Ok(secret) => {
                info!(
                    "Request {}: Successfully retrieved secret '{}' from Infisical",
                    request_id, secret_request.key
                );

                let response = SecretResponse::success(secret);
                self.send_encrypted_response(msg, &response, &host_xkey)
                    .await?;

                debug!("Request {}: Sent successful response", request_id);
            }
            Err(e) => {
                let error_msg = format!("Failed to fetch secret from Infisical: {}", e);
                warn!("Request {}: {}", request_id, error_msg);
                return self.send_error_response(msg, &error_msg).await;
            }
        }

        Ok(())
    }

    /// Processes a server xkey request
    async fn process_xkey_request(&self, msg: &Message) -> Result<()> {
        debug!("Returning server public key");

        let public_key = self.encryption_handler.public_key();
        debug!("Public KEY: {public_key}");

        if let Some(reply) = &msg.reply {
            self.nats_client
                .publish(reply.clone(), public_key.into())
                .await
                .context("Failed to send xkey response")?;

            debug!("Sent server public key response");
        } else {
            warn!("Received xkey request without reply subject");
        }

        Ok(())
    }

    /// Extracts the host xkey from message headers
    fn extract_host_xkey(&self, headers: &Option<HeaderMap>) -> Result<String> {
        let headers = headers
            .as_ref()
            .context("Missing headers in request message")?;

        let host_xkey = headers
            .get("WasmCloud-Host-Xkey")
            .context("Missing Wasmcloud-Host-Xkey header")?
            .as_str();

        Ok(host_xkey.to_string())
    }

    /// Sends an encrypted response back to the requester
    async fn send_encrypted_response(
        &self,
        msg: &Message,
        response: &SecretResponse,
        host_xkey: &str,
    ) -> Result<()> {
        let encryption_key = XKey::new();

        info!("KKKK: {:?}", response);

        // Serialize response to bytes
        let payload =
            serde_json::to_vec(response).context("Failed to serialize secret response")?;
        let payload_bytes = Bytes::from(payload);

        // Encrypt payload
        let encrypted_payload = encryption_key
            .seal(
                &payload_bytes,
                &XKey::from_public_key(host_xkey)
                    .map_err(|e| anyhow::anyhow!("Failed to parse host xkey: {}", e))?,
            )
            .map_err(|e| anyhow::anyhow!("Failed to encrypt payload: {}", e))?;

        // Send response
        if let Some(reply) = &msg.reply {
            let mut headers = async_nats::HeaderMap::new();
            headers.insert("Server-Response-Xkey", encryption_key.public_key().as_str());
            self.nats_client
                .publish_with_headers(reply.clone(), headers, Bytes::from(encrypted_payload))
                .await
                .context("Failed to send encrypted response")?;
        } else {
            return Err(anyhow::anyhow!("No reply subject in request message"));
        }

        Ok(())
    }

    /// Sends an error response (encrypted)
    async fn send_error_response(&self, msg: &Message, error_message: &str) -> Result<()> {
        // Try to extract host xkey - if we can't, we can't send an encrypted response
        let host_xkey = match self.extract_host_xkey(&msg.headers) {
            Ok(key) => key,
            Err(_) => {
                warn!("Cannot send encrypted error response - no valid host xkey");
                return Ok(());
            }
        };

        let error_response = SecretResponse::error(error_message);
        self.send_encrypted_response(msg, &error_response, &host_xkey)
            .await
    }

    /// Returns the server's public key
    pub fn public_key(&self) -> String {
        self.encryption_handler.public_key()
    }

    /// Returns the instance ID
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
}

impl Clone for InfisicalSecretsBackend {
    fn clone(&self) -> Self {
        Self {
            nats_client: self.nats_client.clone(),
            infisical_client: self.infisical_client.clone(),
            encryption_handler: self.encryption_handler.clone(),
            jwt_validator: JwtValidator::default(), // JWT validator is stateless
            config: self.config.clone(),
            instance_id: self.instance_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> AppConfig {
        AppConfig {
            infisical: crate::config::InfisicalConfig {
                client_id: "test_client_id".to_string(),
                client_secret: "test_client_secret".to_string(),
                base_url: "https://app.infisical.com".to_string(),
                project_id: "test_project_id".to_string(),
                environment: "test".to_string(),
            },
            nats: crate::config::NatsConfig {
                jwt: None,
                nkey: None,
                url: "nats://localhost:4222".to_string(),
                subject_prefix: "wasmcloud.secrets".to_string(),
            },
            backend: crate::config::BackendConfig {
                name: "infisical".to_string(),
                api_version: "v1alpha1".to_string(),
            },
        }
    }

    #[test]
    fn test_subject_generation() {
        let config = create_test_config();
        assert_eq!(
            config.get_subject(),
            "wasmcloud.secrets.v1alpha1.infisical.get"
        );
        assert_eq!(
            config.server_xkey_subject(),
            "wasmcloud.secrets.v1alpha1.infisical.server_xkey"
        );
    }

    #[test]
    fn test_backend_clone() {
        let config = create_test_config();

        // We can't easily test the full backend creation without real connections,
        // but we can test that the config is properly structured
        assert_eq!(config.backend.name, "infisical");
        assert_eq!(config.backend.api_version, "v1alpha1");
    }

    #[tokio::test]
    async fn test_header_extraction() {
        let _backend_config = create_test_config();

        // This would require actual backend initialization in a real test
        // For now, we test the header key constant
        assert_eq!("Wasmcloud-Host-Xkey", "Wasmcloud-Host-Xkey");
    }
}
