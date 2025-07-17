use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub railway: RailwayConfig,
    pub service: ServiceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub notification_channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailwayConfig {
    pub environment_id: String,
    pub token: String,
    pub project_id: String,
    pub api_url: String,
    pub default_template_repo: String,
    pub default_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub name_prefix: String,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            name_prefix: "wasmcloud".to_string(),
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = env::var("DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DATABASE_URL environment variable is required"))?;

        let railway_environment_id = env::var("RAILWAY_ENVIRONMENT_ID").map_err(|_| {
            anyhow::anyhow!("RAILWAY_ENVIRONMENT_ID environment variable is required")
        })?;

        let railway_token = env::var("RAILWAY_TOKEN")
            .map_err(|_| anyhow::anyhow!("RAILWAY_TOKEN environment variable is required"))?;

        let railway_project_id = env::var("RAILWAY_PROJECT_ID")
            .map_err(|_| anyhow::anyhow!("RAILWAY_PROJECT_ID environment variable is required"))?;

        let railway_api_url = env::var("RAILWAY_API_URL")
            .unwrap_or_else(|_| "https://backboard.railway.app/graphql/v2".to_string());

        let default_template_repo = env::var("RAILWAY_DEFAULT_TEMPLATE_REPO")
            .unwrap_or_else(|_| "pipestack/wasmcloud-infra".to_string());

        let default_branch =
            env::var("RAILWAY_DEFAULT_BRANCH").unwrap_or_else(|_| "main".to_string());

        let notification_channel = env::var("DATABASE_NOTIFICATION_CHANNEL")
            .unwrap_or_else(|_| "workspace_created".to_string());

        let service_name_prefix =
            env::var("SERVICE_NAME_PREFIX").unwrap_or_else(|_| "wasmcloud".to_string());

        let max_retries = env::var("MAX_RETRIES")
            .unwrap_or_else(|_| "3".to_string())
            .parse()
            .unwrap_or(3);

        let retry_delay_ms = env::var("RETRY_DELAY_MS")
            .unwrap_or_else(|_| "1000".to_string())
            .parse()
            .unwrap_or(1000);

        Ok(Config {
            database: DatabaseConfig {
                url: database_url,
                notification_channel,
            },
            railway: RailwayConfig {
                environment_id: railway_environment_id,
                token: railway_token,
                project_id: railway_project_id,
                api_url: railway_api_url,
                default_template_repo,
                default_branch,
            },
            service: ServiceConfig {
                name_prefix: service_name_prefix,
                max_retries,
                retry_delay_ms,
            },
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.database.url.is_empty() {
            return Err(anyhow::anyhow!("Database URL cannot be empty"));
        }

        if self.railway.token.is_empty() {
            return Err(anyhow::anyhow!("Railway token cannot be empty"));
        }

        if self.railway.project_id.is_empty() {
            return Err(anyhow::anyhow!("Railway project ID cannot be empty"));
        }

        if !self.railway.api_url.starts_with("http") {
            return Err(anyhow::anyhow!("Railway API URL must be a valid HTTP URL"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let mut config = Config {
            database: DatabaseConfig {
                url: "postgresql://test".to_string(),
                notification_channel: "test_channel".to_string(),
            },
            railway: RailwayConfig {
                environment_id: "test_environment".to_string(),
                token: "test_token".to_string(),
                project_id: "test_project".to_string(),
                api_url: "https://api.railway.app".to_string(),
                default_template_repo: "https://github.com/test/repo".to_string(),
                default_branch: "main".to_string(),
            },
            service: ServiceConfig::default(),
        };

        assert!(config.validate().is_ok());

        // Test empty database URL
        config.database.url = "".to_string();
        assert!(config.validate().is_err());

        // Test zero polling interval
        config.railway.api_url = "https://api.railway.app".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_service_config_default() {
        let config = ServiceConfig::default();
        assert_eq!(config.name_prefix, "wasmcloud");
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
    }
}
