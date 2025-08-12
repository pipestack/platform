use crate::builders::ProviderBuilder;
#[cfg(test)]
use crate::builders::ProviderType;
use crate::builders::providers::{
    HttpClientProviderBuilder, HttpServerProviderBuilder, NatsMessagingProviderBuilder,
};

pub struct ProviderBuilderRegistry {
    http_server: HttpServerProviderBuilder,
    http_client: HttpClientProviderBuilder,
    nats_messaging: NatsMessagingProviderBuilder,
}

impl ProviderBuilderRegistry {
    pub fn new() -> Self {
        Self {
            http_server: HttpServerProviderBuilder,
            http_client: HttpClientProviderBuilder,
            nats_messaging: NatsMessagingProviderBuilder,
        }
    }

    #[cfg(test)]
    pub fn get_builder(&self, provider_type: &ProviderType) -> Option<&dyn ProviderBuilder> {
        match provider_type {
            ProviderType::HttpServer => Some(&self.http_server),
            ProviderType::HttpClient => Some(&self.http_client),
            ProviderType::NatsMessaging => Some(&self.nats_messaging),
        }
    }

    pub fn get_all_providers(&self) -> Vec<&dyn ProviderBuilder> {
        vec![
            &self.http_server as &dyn ProviderBuilder,
            &self.http_client as &dyn ProviderBuilder,
            &self.nats_messaging as &dyn ProviderBuilder,
        ]
    }
}

impl Default for ProviderBuilderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
