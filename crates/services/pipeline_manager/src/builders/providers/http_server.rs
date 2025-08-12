use std::collections::BTreeMap;

use crate::builders::{Component, Config, Properties, ProviderBuilder, Trait, TraitProperties};
use crate::settings::Settings;

pub struct HttpServerProviderBuilder;

impl ProviderBuilder for HttpServerProviderBuilder {
    fn build_component(
        &self,
        _workspace_slug: &str,
        _settings: &Settings,
    ) -> Result<Component, Box<dyn std::error::Error>> {
        let mut http_server_config_props = BTreeMap::new();
        http_server_config_props.insert(
            "routing_mode".to_string(),
            serde_yaml::Value::String("path".to_string()),
        );
        http_server_config_props.insert(
            "address".to_string(),
            serde_yaml::Value::String("0.0.0.0:8000".to_string()),
        );

        Ok(Component {
            name: "httpserver".to_string(),
            component_type: "capability".to_string(),
            properties: Properties::WithImage {
                id: None,
                image: "ghcr.io/wasmcloud/http-server:0.27.0".to_string(),
                config: Some(vec![Config {
                    name: "default-http-config".to_string(),
                    properties: http_server_config_props,
                }]),
            },
            traits: vec![Trait {
                trait_type: "spreadscaler".to_string(),
                properties: TraitProperties::Spreadscaler { instances: 1 },
            }],
        })
    }
}
