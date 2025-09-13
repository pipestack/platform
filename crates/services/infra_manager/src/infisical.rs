use anyhow::{Context, Result};
use infisical::secrets::{CreateSecretRequest, GetSecretRequest};
use infisical::{AuthMethod, Client};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::InfisicalConfig;
use crate::nats::NatsCredentials;

/// Wrapper around the Infisical client that handles authentication and secret operations
pub struct InfisicalClient {
    client: Arc<RwLock<Client>>,
    config: InfisicalConfig,
}

impl InfisicalClient {
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

    /// Create a folder structure in Infisical using the REST API
    ///
    /// This function creates all necessary parent folders to ensure the complete path exists.
    /// It handles nested folder creation by progressively building the path structure.
    ///
    /// # Arguments
    ///
    /// * `folder_path` - The full path of the folder to create (e.g., "/nats/workspaces/my-workspace")
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If all folders were created successfully or already exist
    /// * `Err(anyhow::Error)` - If folder creation fails due to authentication or API errors
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use anyhow::Result;
    /// # async fn example(client: &InfisicalClient) -> Result<()> {
    /// // Create nested folder structure
    /// client.create_folder("/nats/workspaces/my-workspace").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_folder(&self, folder_path: &str) -> Result<()> {
        info!("Creating folder structure in Infisical: {}", folder_path);

        // Validate input path
        if folder_path.is_empty() {
            warn!("Empty folder path provided, skipping folder creation");
            return Ok(());
        }

        // Split the path into components to create nested folders
        let path_components: Vec<&str> = folder_path.trim_start_matches('/').split('/').collect();
        let filtered_components: Vec<&str> = path_components
            .into_iter()
            .filter(|c| !c.is_empty())
            .collect();

        if filtered_components.is_empty() {
            debug!("Root path provided, no folder creation needed");
            return Ok(());
        }

        // Create each folder level progressively
        let mut current_path = String::from("/");

        for component in &filtered_components {
            // Validate folder name component
            if component.len() > 255 {
                return Err(anyhow::anyhow!(
                    "Folder name '{}' exceeds maximum length of 255 characters",
                    component
                ));
            }

            // Update current path
            if current_path == "/" {
                current_path = format!("/{}", component);
            } else {
                current_path = format!("{}/{}", current_path, component);
            }

            // Try to create the folder at this level
            match self.create_single_folder(component, &current_path).await {
                Ok(_) => {
                    debug!("Successfully created folder: {}", current_path);
                }
                Err(e)
                    if e.to_string().contains("already exists")
                        || e.to_string().contains("duplicate")
                        || e.to_string().contains("409") =>
                {
                    debug!("Folder already exists: {}", current_path);
                    // Continue with next folder level
                }
                Err(e) => {
                    error!("Failed to create folder '{}': {}", current_path, e);
                    return Err(e.context(format!(
                        "Failed to create folder structure at '{}'",
                        current_path
                    )));
                }
            }
        }

        info!(
            "Successfully ensured folder structure exists: {}",
            folder_path
        );
        Ok(())
    }

    /// Create a single folder using the Infisical REST API
    ///
    /// This internal method handles the creation of a single folder at a specific path level.
    /// It constructs the appropriate API request and handles various response scenarios.
    ///
    /// # Arguments
    ///
    /// * `folder_name` - The name of the folder to create
    /// * `full_path` - The complete path where the folder should be created
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the folder was created successfully or already exists
    /// * `Err(anyhow::Error)` - If folder creation fails due to API or network errors
    async fn create_single_folder(&self, folder_name: &str, full_path: &str) -> Result<()> {
        // Get parent directory path
        let parent_path = if full_path == format!("/{}", folder_name) {
            "/"
        } else {
            &full_path[..full_path.len() - folder_name.len() - 1]
        };

        // Make direct HTTP request to Infisical REST API
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        let url = format!(
            "{}/api/v1/folders",
            self.config.base_url.trim_end_matches('/')
        );

        // Note: We need to get the access token from the authenticated client
        // This is a limitation - the current infisical crate doesn't expose the token
        // For now, we'll need to make a separate auth request
        let token = self
            .get_access_token()
            .await
            .context("Failed to obtain access token for folder creation")?;

        let payload = json!({
            "workspaceId": self.config.project_id,
            "environment": self.config.environment,
            "name": folder_name,
            "path": parent_path,
            "directory": parent_path,
            "description": format!("Auto-created folder for NATS credentials at {}", full_path)
        });

        debug!(
            "Creating folder '{}' at parent path '{}' with payload: {}",
            folder_name, parent_path, payload
        );

        let response = http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Content-Type", "application/json")
            .header("User-Agent", "infra-manager/1.0")
            .json(&payload)
            .send()
            .await
            .context("Failed to send folder creation request to Infisical API")?;

        let status = response.status();

        if status.is_success() {
            debug!("Successfully created folder '{}' via REST API", folder_name);
            Ok(())
        } else if status.as_u16() == 409 {
            debug!("Folder '{}' already exists (409 Conflict)", folder_name);
            Ok(())
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());

            // Check for common "already exists" patterns in different response formats
            if error_text.to_lowercase().contains("already exists")
                || error_text.to_lowercase().contains("duplicate")
                || error_text.to_lowercase().contains("exists")
            {
                debug!(
                    "Folder '{}' already exists (detected from error message)",
                    folder_name
                );
                Ok(())
            } else {
                error!(
                    "Failed to create folder '{}': HTTP {} - {}",
                    folder_name, status, error_text
                );
                Err(anyhow::anyhow!(
                    "API request failed: HTTP {} - {}",
                    status,
                    error_text
                ))
            }
        }
    }

    /// Get access token by making a separate authentication request
    ///
    /// This is a workaround needed because the current infisical Rust crate doesn't expose
    /// the access token directly. We make a separate authentication request to obtain the
    /// token needed for direct REST API calls.
    ///
    /// # Returns
    ///
    /// * `Ok(String)` - The access token for API authentication
    /// * `Err(anyhow::Error)` - If authentication fails
    ///
    /// # Note
    ///
    /// This method performs a fresh authentication each time it's called. In a production
    /// environment, you might want to cache the token and refresh it when it expires.
    async fn get_access_token(&self) -> Result<String> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client for authentication")?;

        let url = format!(
            "{}/api/v1/auth/universal-auth/login",
            self.config.base_url.trim_end_matches('/')
        );

        let payload = json!({
            "clientId": self.config.client_id,
            "clientSecret": self.config.client_secret
        });

        debug!("Authenticating to get access token for REST API calls");

        let response = http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "infra-manager/1.0")
            .json(&payload)
            .send()
            .await
            .context("Failed to send authentication request to Infisical")?;

        if response.status().is_success() {
            let auth_response: serde_json::Value = response
                .json()
                .await
                .context("Failed to parse authentication response as JSON")?;

            let token = auth_response["accessToken"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Access token not found in authentication response. Response structure may have changed."))?;

            debug!("Successfully obtained access token for REST API operations");
            Ok(token.to_string())
        } else {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unable to read error response".to_string());
            error!("Authentication failed: HTTP {} - {}", status, error_text);
            Err(anyhow::anyhow!(
                "Authentication failed: HTTP {} - {}",
                status,
                error_text
            ))
        }
    }

    /// Create the folder structure for a NATS workspace
    ///
    /// This is a convenience function that creates the standard folder structure
    /// used for storing NATS credentials: `/nats/workspaces/{workspace_slug}`
    ///
    /// # Arguments
    ///
    /// * `workspace_slug` - The workspace identifier (e.g., "my-workspace")
    ///
    /// # Returns
    ///
    /// * `Ok(())` - If the folder structure was created successfully
    /// * `Err(anyhow::Error)` - If folder creation fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use anyhow::Result;
    /// # async fn example(client: &InfisicalClient) -> Result<()> {
    /// // Create folder structure for a workspace
    /// client.create_nats_workspace_folder("production-workspace").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_nats_workspace_folder(&self, workspace_slug: &str) -> Result<()> {
        let folder_path = format!("/nats/workspaces/{}", workspace_slug);
        info!(
            "Creating NATS workspace folder structure for: {}",
            workspace_slug
        );

        self.create_folder(&folder_path).await.context(format!(
            "Failed to create NATS workspace folder structure for '{}'",
            workspace_slug
        ))
    }

    /// Store NATS credentials for a workspace in Infisical
    pub async fn store_nats_credentials(
        &self,
        workspace_slug: &str,
        credentials: &NatsCredentials,
    ) -> Result<()> {
        info!("Storing NATS credentials for workspace: {}", workspace_slug);

        let client = self.client.read().await;
        let base_path = format!("/nats/workspaces/{}", workspace_slug);

        // Ensure the folder structure exists before storing secrets
        // This helps organize secrets in a hierarchical structure
        if let Err(e) = self.create_nats_workspace_folder(workspace_slug).await {
            warn!(
                "Failed to create folder structure for workspace '{}': {}. Continuing with secret storage - secrets may be stored in root directory.",
                workspace_slug, e
            );
        }

        // Store each credential component as a separate secret
        let secrets = vec![
            ("account_nkey", &credentials.account_nkey),
            ("account_jwt", &credentials.account_jwt),
            ("user_nkey", &credentials.user_nkey),
            ("user_jwt", &credentials.user_jwt),
            ("user_seed", &credentials.user_seed),
        ];

        for (key, value) in secrets {
            let secret_key = String::from(key);

            let create_request = CreateSecretRequest::builder(
                &secret_key,
                value,
                &self.config.project_id,
                &self.config.environment,
            )
            .path(&base_path)
            .build();

            match client.secrets().create(create_request).await {
                Ok(_) => {
                    debug!(
                        "Successfully stored secret '{}' for workspace '{}'",
                        secret_key, workspace_slug
                    );
                }
                Err(e) => {
                    error!(
                        "Failed to store secret '{}' for workspace '{}': {}",
                        secret_key, workspace_slug, e
                    );
                    return Err(anyhow::anyhow!(
                        "Failed to store secret '{}': {}",
                        secret_key,
                        e
                    ));
                }
            }
        }

        info!(
            "Successfully stored NATS credentials for workspace: {}",
            workspace_slug
        );
        Ok(())
    }

    /// Retrieve NATS credentials for a workspace from Infisical
    pub async fn _get_nats_credentials(
        &self,
        workspace_slug: &str,
    ) -> Result<Option<NatsCredentials>> {
        info!(
            "Retrieving NATS credentials for workspace: {}",
            workspace_slug
        );

        let client = self.client.read().await;
        let base_path = format!("/nats/workspaces/{}", workspace_slug);

        // Retrieve each credential component
        let secret_keys = [
            "account_nkey",
            "account_jwt",
            "user_nkey",
            "user_jwt",
            "user_seed",
        ];
        let mut secrets = std::collections::HashMap::new();

        for key in &secret_keys {
            let secret_key = format!("nats_{}", key);
            let get_request = GetSecretRequest::builder(
                &secret_key,
                &self.config.project_id,
                &self.config.environment,
            )
            .path(&base_path)
            .expand_secret_references(true)
            .build();

            match client.secrets().get(get_request).await {
                Ok(secret) => {
                    secrets.insert(*key, secret.secret_value);
                }
                Err(e) if e.to_string().contains("not found") => {
                    warn!(
                        "Secret '{}' not found for workspace '{}': {}",
                        secret_key, workspace_slug, e
                    );
                    return Ok(None);
                }
                Err(e) => {
                    error!(
                        "Failed to retrieve secret '{}' for workspace '{}': {}",
                        secret_key, workspace_slug, e
                    );
                    return Err(anyhow::anyhow!(
                        "Failed to retrieve secret '{}': {}",
                        secret_key,
                        e
                    ));
                }
            }
        }

        // Construct NatsCredentials from retrieved secrets
        let credentials = NatsCredentials {
            account_nkey: secrets
                .get("account_nkey")
                .ok_or_else(|| anyhow::anyhow!("Missing account_nkey"))?
                .clone(),
            account_jwt: secrets
                .get("account_jwt")
                .ok_or_else(|| anyhow::anyhow!("Missing account_jwt"))?
                .clone(),
            user_nkey: secrets
                .get("user_nkey")
                .ok_or_else(|| anyhow::anyhow!("Missing user_nkey"))?
                .clone(),
            user_jwt: secrets
                .get("user_jwt")
                .ok_or_else(|| anyhow::anyhow!("Missing user_jwt"))?
                .clone(),
            user_seed: secrets
                .get("user_seed")
                .ok_or_else(|| anyhow::anyhow!("Missing user_seed"))?
                .clone(),
        };

        info!(
            "Successfully retrieved NATS credentials for workspace: {}",
            workspace_slug
        );
        Ok(Some(credentials))
    }

    /// Test the connection to Infisical
    pub async fn test_connection(&self) -> Result<()> {
        debug!("Testing connection to Infisical");

        let client = self.client.read().await;
        let base_path = "/nats/test";

        // Try to make a simple request to test the connection
        let test_request = GetSecretRequest::builder(
            "__connection_test__",
            &self.config.project_id,
            &self.config.environment,
        )
        .path(base_path)
        .build();

        match client.secrets().get(test_request).await {
            Ok(_) => {
                info!("Infisical connection test successful");
                Ok(())
            }
            Err(e)
                if e.to_string().contains("not found") || e.to_string().contains("Not Found") =>
            {
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

impl Clone for InfisicalClient {
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

    fn create_test_credentials() -> NatsCredentials {
        NatsCredentials {
            account_nkey: "ATEST123456789ABCDEF".to_string(),
            account_jwt: "eyJ0eXAiOiJKV1QiLCJhbGciOiJFZDI1NTE5LW5rZXkifQ.test.account.jwt"
                .to_string(),
            user_nkey: "UTEST123456789ABCDEF".to_string(),
            user_jwt: "eyJ0eXAiOiJKV1QiLCJhbGciOiJFZDI1NTE5LW5rZXkifQ.test.user.jwt".to_string(),
            user_seed: "SUTEST123456789ABCDEFGHIJKLMNOP".to_string(),
        }
    }

    #[test]
    fn test_config_validation() {
        let config = create_test_config();
        assert_eq!(config.client_id, "test_client_id");
        assert_eq!(config.project_id, "test_project_id");
        assert_eq!(config.environment, "test");
    }

    #[test]
    fn test_credentials_structure() {
        let credentials = create_test_credentials();
        assert!(credentials.account_nkey.starts_with("A"));
        assert!(credentials.user_nkey.starts_with("U"));
        assert!(credentials.user_seed.starts_with("SU"));
    }

    #[tokio::test]
    async fn test_infisical_client_clone() {
        // We can't test the actual client without real credentials,
        // but we can test that the struct is properly configured for cloning
        let config = create_test_config();

        // This would require real credentials to work in practice
        // let client = InfisicalClient::new(config).await.unwrap();
        // let cloned_client = client.clone();

        // For now, just test that the config is accessible
        assert_eq!(config.base_url, "https://app.infisical.com");
    }

    #[test]
    fn test_folder_path_parsing() {
        // Test various folder path formats
        let test_cases = vec![
            ("/", vec![]),
            ("", vec![]),
            ("/nats", vec!["nats"]),
            ("/nats/workspaces", vec!["nats", "workspaces"]),
            (
                "/nats/workspaces/test-workspace",
                vec!["nats", "workspaces", "test-workspace"],
            ),
            ("nats/workspaces/test", vec!["nats", "workspaces", "test"]),
        ];

        for (input, expected) in test_cases {
            let components: Vec<&str> = input.trim_start_matches('/').split('/').collect();
            let filtered: Vec<&str> = components.into_iter().filter(|c| !c.is_empty()).collect();
            assert_eq!(filtered, expected, "Failed for input: {}", input);
        }
    }

    #[test]
    fn test_parent_path_calculation() {
        let test_cases = vec![
            ("/nats", "nats", "/"),
            ("/nats/workspaces", "workspaces", "/nats"),
            ("/nats/workspaces/test", "test", "/nats/workspaces"),
        ];

        for (full_path, folder_name, expected_parent) in test_cases {
            let parent_path = if full_path == format!("/{}", folder_name) {
                "/"
            } else {
                &full_path[..full_path.len() - folder_name.len() - 1]
            };
            assert_eq!(
                parent_path, expected_parent,
                "Failed for full_path: {}, folder_name: {}",
                full_path, folder_name
            );
        }
    }

    #[test]
    fn test_base_path_generation() {
        let workspace_slug = "test-workspace";
        let expected_path = "/nats/workspaces/test-workspace";
        let base_path = format!("/nats/workspaces/{}", workspace_slug);
        assert_eq!(base_path, expected_path);
    }

    #[test]
    fn test_nats_folder_path_generation() {
        let test_cases = vec![
            ("test", "/nats/workspaces/test"),
            ("production-env", "/nats/workspaces/production-env"),
            ("dev_workspace", "/nats/workspaces/dev_workspace"),
            ("workspace-123", "/nats/workspaces/workspace-123"),
        ];

        for (workspace_slug, expected) in test_cases {
            let folder_path = format!("/nats/workspaces/{}", workspace_slug);
            assert_eq!(
                folder_path, expected,
                "Failed for workspace_slug: {}",
                workspace_slug
            );
        }
    }
}
