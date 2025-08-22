use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

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
    pub jwt: String,
    pub nkey: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Registry {
    pub url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Settings {
    pub cloudflare: Cloudflare,
    pub nats: Nats,
    pub registry: Registry,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let s = Config::builder()
            .add_source(File::with_name(".env.local").required(false))
            .add_source(Environment::with_prefix("pipestack").separator("__"))
            .build()?;
        let settings: Settings = s.try_deserialize()?;
        Ok(settings)
    }
}
