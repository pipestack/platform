use std::collections::BTreeMap;

use crate::builders::{Component, Config, Properties, ProviderBuilder, Trait, TraitProperties};
use crate::config::AppConfig;

pub struct NatsMessagingProviderBuilder;

impl ProviderBuilder for NatsMessagingProviderBuilder {
    fn build_component(
        &self,
        workspace_slug: &str,
        app_config: &AppConfig,
    ) -> Result<Component, Box<dyn std::error::Error>> {
        Ok(Component {
            name: "messaging-nats".to_string(),
            component_type: "capability".to_string(),
            properties: Properties::WithImage {
                id: None,
                image: "ghcr.io/wasmcloud/messaging-nats:0.27.0".to_string(),
                config: Some(vec![Config {
                    name: format!("{workspace_slug}-messaging-nats-config"),
                    properties: {
                        let mut props = BTreeMap::new();
                        props.insert(
                            "cluster_uris".to_string(),
                            serde_yaml::Value::String(app_config.nats.cluster_uris.to_string()),
                        );
                        if let Some(jwt) = &app_config.nats.jwt {
                            props.insert(
                                "client_jwt".to_string(),
                                serde_yaml::Value::String(jwt.clone()),
                            );
                        }
                        if let Some(seed) = &app_config.nats.nkey {
                            props.insert(
                                "client_seed".to_string(),
                                serde_yaml::Value::String(seed.clone()),
                            );
                        }
                        props
                    },
                }]),
            },
            traits: vec![Trait {
                trait_type: "spreadscaler".to_string(),
                properties: TraitProperties::Spreadscaler { instances: 1 },
            }],
        })
    }
}
