use crate::builders::{
    BuildContext, Component, ComponentBuilder, LinkProperties, LinkTarget, Properties, Trait,
    TraitProperties,
};
use shared::PipelineNode;

pub struct OutLogBuilder;

impl ComponentBuilder for OutLogBuilder {
    fn build_components(
        &self,
        step: &PipelineNode,
        context: &BuildContext,
    ) -> Result<Vec<Component>, Box<dyn std::error::Error>> {
        let mut components = Vec::new();

        // Add in-internal component for out-log
        components.push(Component {
            name: format!("in-internal-for-{}", step.name),
            component_type: "component".to_string(),
            properties: Properties::WithImage {
                id: Some(format!(
                    "{}_{}-in-internal-for-{}",
                    context.workspace_slug, context.pipeline.name, step.name
                )),
                image: format!(
                    "{}/pipestack/in-internal:0.0.1",
                    context.settings.registry.url
                ),
                config: None,
            },
            traits: vec![
                Trait {
                    trait_type: "spreadscaler".to_string(),
                    properties: TraitProperties::Spreadscaler { instances: 1 },
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

        // Add the out-log component itself
        components.push(Component {
            name: step.name.clone(),
            component_type: "component".to_string(),
            properties: Properties::WithImage {
                id: Some(format!(
                    "{}_{}-{}",
                    context.workspace_slug, context.pipeline.name, step.name
                )),
                image: format!("{}/pipestack/out-log:0.0.1", context.settings.registry.url),
                config: None,
            },
            traits: vec![Trait {
                trait_type: "spreadscaler".to_string(),
                properties: TraitProperties::Spreadscaler {
                    instances: step.instances.unwrap_or(1),
                },
            }],
        });

        Ok(components)
    }
}
