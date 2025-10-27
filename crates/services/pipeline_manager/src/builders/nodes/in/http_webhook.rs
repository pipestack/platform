use crate::builders::{
    BuildContext, Component, ComponentBuilder, Config, LinkProperties, LinkTarget, Properties,
    Trait, TraitProperties, nodes::NODE_IN_HTTP_NAME, nodes::NODE_IN_HTTP_VERSION,
    nodes::NODE_OUT_INTERNAL_NAME, nodes::NODE_OUT_INTERNAL_VERSION, settings_to_config_properties,
};
use shared::{PipelineNode, PipelineNodeSettings};

pub struct InHttpWebhookBuilder;

impl ComponentBuilder for InHttpWebhookBuilder {
    fn build_components(
        &self,
        step: &PipelineNode,
        context: &BuildContext,
    ) -> Result<Vec<Component>, Box<dyn std::error::Error>> {
        let mut components = Vec::new();

        // Add in-http component
        components.push(Component {
            name: step.id.clone(),
            component_type: "component".to_string(),
            properties: Properties::WithImage {
                id: Some(format!(
                    "{}_{}-{}",
                    context.workspace_slug, context.pipeline.name, step.id
                )),
                image: format!(
                    "{}/nodes/{NODE_IN_HTTP_NAME}:{NODE_IN_HTTP_VERSION}",
                    context.app_config.registry.url
                ),
                config: step.settings.as_ref().map(|s| match s {
                    PipelineNodeSettings::InHttpWebhook(settings) => vec![Config {
                        name: format!("{}-config-v{}", step.id, context.pipeline.version),
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
                            name: format!("out-internal-for-{}", step.id),
                            config: None,
                        },
                        namespace: "pipestack".to_string(),
                        package: "out".to_string(),
                        interfaces: vec!["out".to_string()],
                    }),
                },
            ],
        });

        // Add corresponding out-internal component
        let next_topic = context.find_next_step_topic(&step.id).unwrap_or_default();

        if !next_topic.is_empty() {
            components.push(Component {
                name: format!("out-internal-for-{}", step.id),
                component_type: "component".to_string(),
                properties: Properties::WithImage {
                    id: Some(format!(
                        "{}_{}-out-internal-for-{}",
                        context.workspace_slug, context.pipeline.name, step.id
                    )),
                    image: format!(
                        "{}/nodes/{NODE_OUT_INTERNAL_NAME}:{NODE_OUT_INTERNAL_VERSION}",
                        context.app_config.registry.url
                    ),
                    config: Some(vec![Config {
                        name: format!(
                            "out-internal-for-{}-config-v{}",
                            step.id, context.pipeline.version
                        ),
                        properties: {
                            let mut props = std::collections::BTreeMap::new();
                            props.insert(
                                "next-step-topic".to_string(),
                                serde_yaml::Value::String(next_topic.clone()),
                            );
                            props
                        },
                    }]),
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
                ],
            });
        }

        Ok(components)
    }
}
