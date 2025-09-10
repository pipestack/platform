use anyhow::Result;
use config::{Config, Environment, File};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AppConfig {
    pub infisical: InfisicalConfig,
    pub nats: NatsConfig,
    pub backend: BackendConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InfisicalConfig {
    pub client_id: String,
    pub client_secret: String,
    pub base_url: String,
    pub project_id: String,
    pub environment: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NatsConfig {
    pub jwt: Option<String>,
    pub nkey: Option<String>,
    pub url: String,
    pub subject_prefix: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackendConfig {
    pub name: String,
    pub api_version: String,
}

impl Default for InfisicalConfig {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret: String::new(),
            base_url: "https://app.infisical.com".to_string(),
            project_id: String::new(),
            environment: "prod".to_string(),
        }
    }
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            jwt: None,
            nkey: None,
            url: "nats://localhost:4222".to_string(),
            subject_prefix: "wasmcloud.secrets".to_string(),
        }
    }
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            name: "infisical".to_string(),
            api_version: "v1alpha1".to_string(),
        }
    }
}

impl AppConfig {
    pub fn new() -> Result<Self> {
        let defaults = Config::try_from(&AppConfig::default())?;
        let c = Config::builder()
            .add_source(defaults)
            .add_source(File::with_name(".env.local").required(false))
            .add_source(Environment::with_prefix("pipestack").separator("__"))
            .build()?;
        let app_config: AppConfig = c.try_deserialize()?;
        tracing::debug!("Loaded app config: {:?}", app_config);
        Ok(app_config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.infisical.client_id.is_empty() {
            return Err(anyhow::anyhow!("Infisical client_id cannot be empty"));
        }

        if self.infisical.client_secret.is_empty() {
            return Err(anyhow::anyhow!("Infisical client_secret cannot be empty"));
        }

        if self.infisical.project_id.is_empty() {
            return Err(anyhow::anyhow!("Infisical project_id cannot be empty"));
        }

        if self.nats.url.is_empty() {
            return Err(anyhow::anyhow!("NATS URL cannot be empty"));
        }

        if self.backend.name.is_empty() {
            return Err(anyhow::anyhow!("Backend name cannot be empty"));
        }

        Ok(())
    }

    /// Returns the NATS subject for the get operation
    pub fn get_subject(&self) -> String {
        format!(
            "{}.{}.{}.get",
            self.nats.subject_prefix, self.backend.api_version, self.backend.name
        )
    }

    /// Returns the NATS subject for the server_xkey operation
    pub fn server_xkey_subject(&self) -> String {
        format!(
            "{}.{}.{}.server_xkey",
            self.nats.subject_prefix, self.backend.api_version, self.backend.name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subject_generation() {
        let config = AppConfig::default();
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
    fn test_validation() {
        let mut config = AppConfig::default();

        // Should fail validation with empty required fields
        assert!(config.validate().is_err());

        // Fill required fields
        config.infisical.client_id = "test_client_id".to_string();
        config.infisical.client_secret = "test_client_secret".to_string();
        config.infisical.project_id = "test_project_id".to_string();

        // Should pass validation now
        assert!(config.validate().is_ok());
    }
}
