use crate::builders::{Component, Properties, ProviderBuilder, Trait, TraitProperties};
use crate::settings::Settings;

pub struct HttpClientProviderBuilder;

impl ProviderBuilder for HttpClientProviderBuilder {
    fn build_component(
        &self,
        _workspace_slug: &str,
        _settings: &Settings,
    ) -> Result<Component, Box<dyn std::error::Error>> {
        Ok(Component {
            name: "httpclient".to_string(),
            component_type: "capability".to_string(),
            properties: Properties::WithImage {
                id: None,
                image: "ghcr.io/wasmcloud/http-client:0.13.1".to_string(),
                config: None,
            },
            traits: vec![Trait {
                trait_type: "spreadscaler".to_string(),
                properties: TraitProperties::Spreadscaler { instances: 1 },
            }],
        })
    }
}
