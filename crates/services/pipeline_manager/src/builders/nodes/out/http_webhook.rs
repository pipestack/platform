use crate::builders::{
    BuildContext, Component, ComponentBuilder, Config, LinkProperties, LinkTarget, Properties,
    Trait, TraitProperties, nodes::NODE_VERSION_IN_INTERNAL, nodes::NODE_VERSION_OUT_HTTP_WEBHOOK,
    settings_to_config_properties,
};
use shared::{PipelineNode, PipelineNodeSettings};

pub struct OutHttpWebhookBuilder;

impl ComponentBuilder for OutHttpWebhookBuilder {
    fn build_components(
        &self,
        step: &PipelineNode,
        context: &BuildContext,
    ) -> Result<Vec<Component>, Box<dyn std::error::Error>> {
        let mut components = Vec::new();

        // Add in-internal component for out-http-webhook
        components.push(Component {
            name: format!("in-internal-for-{}", step.name),
            component_type: "component".to_string(),
            properties: Properties::WithImage {
                id: Some(format!(
                    "{}_{}-in-internal-for-{}",
                    context.workspace_slug, context.pipeline.name, step.name
                )),
                image: format!(
                    "{}/nodes/in_internal:{NODE_VERSION_IN_INTERNAL}",
                    context.app_config.registry.url
                ),
                config: None,
            },
            traits: vec![
                Trait {
                    trait_type: "spreadscaler".to_string(),
                    properties: TraitProperties::Spreadscaler { instances: 10_000 },
                },
                Trait {
                    trait_type: "link".to_string(),
                    properties: TraitProperties::Link(LinkProperties {
                        name: None,
                        source: None,
                        target: LinkTarget {
                            name: "messaging-nats".to_string(),
                            config: None,
                        },
                        namespace: "wasmcloud".to_string(),
                        package: "messaging".to_string(),
                        interfaces: vec!["consumer".to_string()],
                    }),
                },
                Trait {
                    trait_type: "link".to_string(),
                    properties: TraitProperties::Link(LinkProperties {
                        name: None,
                        source: None,
                        target: LinkTarget {
                            name: step.name.clone(),
                            config: None,
                        },
                        namespace: "pipestack".to_string(),
                        package: "out".to_string(),
                        interfaces: vec!["out".to_string()],
                    }),
                },
            ],
        });

        // Add the out-http-webhook component itself
        components.push(Component {
            name: step.name.clone(),
            component_type: "component".to_string(),
            properties: Properties::WithImage {
                id: Some(format!(
                    "{}_{}-{}",
                    context.workspace_slug, context.pipeline.name, step.name
                )),
                image: format!(
                    "{}/nodes/out_http_webhook:{NODE_VERSION_OUT_HTTP_WEBHOOK}",
                    context.app_config.registry.url
                ),
                config: step.settings.as_ref().map(|s| match s {
                    PipelineNodeSettings::OutHttpWebhook(settings) => vec![Config {
                        name: format!("{}-config-v{}", step.name, context.pipeline.version),
                        properties: settings_to_config_properties(settings),
                    }],
                    _ => vec![],
                }),
            },
            traits: vec![
                Trait {
                    trait_type: "spreadscaler".to_string(),
                    properties: TraitProperties::Spreadscaler {
                        instances: step.instances.unwrap_or(10_000),
                    },
                },
                Trait {
                    trait_type: "link".to_string(),
                    properties: TraitProperties::Link(LinkProperties {
                        name: None,
                        source: None,
                        target: LinkTarget {
                            name: "httpclient".to_string(),
                            config: None,
                        },
                        namespace: "wasi".to_string(),
                        package: "http".to_string(),
                        interfaces: vec!["outgoing-handler".to_string()],
                    }),
                },
            ],
        });

        Ok(components)
    }
}
