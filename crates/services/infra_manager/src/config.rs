use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub notification_channel: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RailwayConfig {
    pub environment_id: String,
    pub token: String,
    pub project_id: String,
    pub api_url: String,
    pub default_template_repo: String,
    pub default_branch: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServiceConfig {
    pub name_prefix: String,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[derive(Default)]
pub struct AppConfig {
    pub database: DatabaseConfig,
    pub railway: RailwayConfig,
    pub service: ServiceConfig,
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

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            notification_channel: "workspace_created".to_string(),
        }
    }
}

impl Default for RailwayConfig {
    fn default() -> Self {
        Self {
            environment_id: String::new(),
            token: String::new(),
            project_id: String::new(),
            api_url: "https://backboard.railway.app/graphql/v2".to_string(),
            default_template_repo: "pipestack/wasmcloud-infra".to_string(),
            default_branch: "main".to_string(),
        }
    }
}


impl AppConfig {
    pub fn new() -> Result<Self, ConfigError> {
        let s = Config::builder()
            .add_source(File::with_name(".env.local").required(false))
            .add_source(Environment::with_prefix("pipestack").separator("__"))
            .build()?;
        let app_config: AppConfig = s.try_deserialize()?;
        tracing::debug!("Loaded app config: {:?}", app_config);
        Ok(app_config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.database.url.is_empty() {
            return Err(ConfigError::Message(
                "Database URL cannot be empty".to_string(),
            ));
        }

        if self.railway.token.is_empty() {
            return Err(ConfigError::Message(
                "Railway token cannot be empty".to_string(),
            ));
        }

        if self.railway.project_id.is_empty() {
            return Err(ConfigError::Message(
                "Railway project ID cannot be empty".to_string(),
            ));
        }

        if !self.railway.api_url.starts_with("http") {
            return Err(ConfigError::Message(
                "Railway API URL must be a valid HTTP URL".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let mut app_config = AppConfig {
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

        assert!(app_config.validate().is_ok());

        // Test empty database URL
        app_config.database.url = "".to_string();
        assert!(app_config.validate().is_err());

        // Reset database URL and test empty railway token
        app_config.database.url = "postgresql://test".to_string();
        app_config.railway.token = "".to_string();
        assert!(app_config.validate().is_err());
    }

    #[test]
    fn test_service_config_default() {
        let service_config = ServiceConfig::default();
        assert_eq!(service_config.name_prefix, "wasmcloud");
        assert_eq!(service_config.max_retries, 3);
        assert_eq!(service_config.retry_delay_ms, 1000);
    }

    #[test]
    fn test_database_config_default() {
        let database_config = DatabaseConfig::default();
        assert_eq!(database_config.notification_channel, "workspace_created");
        assert!(database_config.url.is_empty());
    }

    #[test]
    fn test_railway_config_default() {
        let railway_config = RailwayConfig::default();
        assert_eq!(
            railway_config.api_url,
            "https://backboard.railway.app/graphql/v2"
        );
        assert_eq!(
            railway_config.default_template_repo,
            "pipestack/wasmcloud-infra"
        );
        assert_eq!(railway_config.default_branch, "main");
    }
}
