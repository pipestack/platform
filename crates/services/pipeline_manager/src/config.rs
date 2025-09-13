use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Cloudflare {
    pub account_id: String,
    pub r2_access_key_id: String,
    pub r2_secret_access_key: String,
    pub r2_bucket: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Nats {
    pub cluster_uris: String,
    pub jwt: Option<String>,
    pub nkey: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Registry {
    pub url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AppConfig {
    pub cloudflare: Cloudflare,
    pub nats: Nats,
    pub registry: Registry,
    pub database: DatabaseConfig,
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
}
