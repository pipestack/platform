use anyhow::{Context, Result};
use infisical::secrets::GetSecretRequest;
use infisical::{AuthMethod, Client};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::InfisicalConfig;
#[cfg(test)]
use crate::types::Context as SecretContext;
use crate::types::{Secret, SecretRequest};

/// Wrapper around the Infisical client that handles authentication and secret retrieval
pub struct InfisicalClientWrapper {
    client: Arc<RwLock<Client>>,
    config: InfisicalConfig,
}

impl InfisicalClientWrapper {
    /// Creates a new Infisical client wrapper
    pub async fn new(config: InfisicalConfig) -> Result<Self> {
        info!(
            "Initializing Infisical client for base URL: {}",
            config.base_url
        );

        let mut client = Client::builder()
            .base_url(&config.base_url)
            .build()
            .await
            .context("Failed to build Infisical client")?;

        // Authenticate with Infisical
        let auth_method = AuthMethod::new_universal_auth(&config.client_id, &config.client_secret);

        debug!("Authenticating with Infisical using Universal Auth");
        client
            .login(auth_method)
            .await
            .context("Failed to authenticate with Infisical")?;

        info!("Successfully authenticated with Infisical");

        Ok(Self {
            client: Arc::new(RwLock::new(client)),
            config,
        })
    }

    /// Retrieves a secret from Infisical
    pub async fn get_secret(&self, request: &SecretRequest) -> Result<Secret> {
        debug!("Fetching secret '{}' from Infisical", request.key);

        let client = self.client.read().await;

        let infisical_request = GetSecretRequest::builder(
            &request.key,
            &self.config.project_id,
            &self.config.environment,
        )
        .path("/nats/workspaces/default") // Default path - could be made configurable in the future
        .expand_secret_references(true)
        .build();

        match client.secrets().get(infisical_request).await {
            Ok(infisical_secret) => {
                debug!(
                    "Successfully retrieved secret '{}' from Infisical",
                    request.key
                );

                Ok(Secret::new_string(
                    infisical_secret.secret_key,
                    infisical_secret.secret_value,
                    request
                        .version
                        .clone()
                        .unwrap_or_else(|| "latest".to_string()),
                ))
            }
            Err(e) if e.to_string().contains("not found") => {
                warn!("Secret '{}' not found in Infisical project", request.key);
                Err(anyhow::anyhow!("Secret '{}' not found", request.key))
            }
            Err(e)
                if e.to_string().contains("unauthorized")
                    || e.to_string().contains("Unauthorized") =>
            {
                error!("Unauthorized access to Infisical - check credentials");
                Err(anyhow::anyhow!("Unauthorized access to Infisical"))
            }
            Err(e) if e.to_string().contains("network") || e.to_string().contains("connection") => {
                error!("Network error while fetching secret from Infisical: {}", e);
                Err(anyhow::anyhow!("Network error: {}", e))
            }
            Err(e) => {
                error!(
                    "Unexpected error while fetching secret from Infisical: {}",
                    e
                );
                Err(anyhow::anyhow!("Infisical error: {}", e))
            }
        }
    }

    /// Tests the connection to Infisical by attempting to list secrets
    pub async fn test_connection(&self) -> Result<()> {
        debug!("Testing connection to Infisical");

        let client = self.client.read().await;

        // Try to make a simple request to test the connection
        let test_request = GetSecretRequest::builder(
            "__test_connection__", // This should not exist
            &self.config.project_id,
            &self.config.environment,
        )
        .path("/")
        .build();

        match client.secrets().get(test_request).await {
            Ok(_) => {
                info!("Infisical connection test successful");
                Ok(())
            }
            Err(e) if e.to_string().contains("Not Found") => {
                // This is expected - connection is working
                info!("Infisical connection test successful (secret not found as expected)");
                Ok(())
            }
            Err(e)
                if e.to_string().contains("unauthorized")
                    || e.to_string().contains("Unauthorized") =>
            {
                error!("Infisical connection test failed - unauthorized");
                Err(anyhow::anyhow!("Unauthorized access to Infisical"))
            }
            Err(e) => {
                error!("Infisical connection test failed: {}", e);
                Err(anyhow::anyhow!("Connection test failed: {}", e))
            }
        }
    }
}

impl Clone for InfisicalClientWrapper {
    fn clone(&self) -> Self {
        Self {
            client: Arc::clone(&self.client),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> InfisicalConfig {
        InfisicalConfig {
            client_id: "test_client_id".to_string(),
            client_secret: "test_client_secret".to_string(),
            base_url: "https://app.infisical.com".to_string(),
            project_id: "test_project_id".to_string(),
            environment: "test".to_string(),
        }
    }

    #[test]
    fn test_config_access() {
        let config = create_test_config();
        let expected_project_id = config.project_id.clone();

        // We can't easily test the async new() method without real credentials,
        // but we can test that the config is stored correctly
        // This would require mocking the Infisical client in a real implementation
        assert_eq!(expected_project_id, "test_project_id");
    }

    #[tokio::test]
    async fn test_secret_request_creation() {
        let request = SecretRequest {
            key: "test_secret".to_string(),
            field: None,
            version: Some("1.0".to_string()),
            context: SecretContext {
                entity_jwt: "test.entity.jwt".to_string(),
                host_jwt: "test.host.jwt".to_string(),
                application: crate::types::Application {
                    name: "test-app".to_string(),
                    policy: "{}".to_string(),
                },
            },
        };

        assert_eq!(request.key, "test_secret");
        assert_eq!(request.version, Some("1.0".to_string()));
    }
}
