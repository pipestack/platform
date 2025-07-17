use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use ts_rs::TS;

use crate::settings::Settings;

const PIPELINE_TS_FILE_PATH: &str = "./pipeline.ts";

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH, optional_fields)]
pub struct Pipeline {
    pub name: String,
    pub version: String,
    pub nodes: Vec<PipelineNode>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH)]
pub struct XYPosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH, optional_fields)]
pub struct PipelineNode {
    pub name: String,
    #[serde(rename = "type")]
    pub step_type: PipelineNodeType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instances: Option<u32>,
    pub position: XYPosition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, rename = "NodeType", export_to = PIPELINE_TS_FILE_PATH)]
pub enum PipelineNodeType {
    // ####################
    // Sources
    // ####################
    //
    // Cloud Storages
    InAwsS3,
    InGoogleGcs,
    InAzureBlob,
    // Databases
    InPostgresql,
    InMongodb,
    InMysql,
    InSqlite,
    // Streaming
    InKafka,
    InNats,
    InRabbitmq,
    InRedis,
    // Web / API
    InHttpWebhook,
    InHttpPoller,
    InGraphqlPoller,
    InRssReader,
    // Cloud Services
    InGooglePubsub,
    InAwsKinesis,
    InStripe,
    InGithubWebhook,
    // ####################
    // Processor nodes
    // ####################
    //
    // Custom
    ProcessorWasm,
    // ####################
    // Sink nodes
    // ####################
    //
    // Databases
    OutPostgresql,
    OutMongodb,
    OutMysql,
    OutRedis,
    // Cloud Storages
    OutAwsS3,
    OutGoogleGcs,
    OutAzureBlob,
    // Streaming / Queues
    OutKafka,
    OutNats,
    OutRabbitmq,
    OutGooglePubsub,
    // Web / API
    OutHttpPost,
    OutGraphqlMutation,
    OutSlack,
    OutTwilioSms,
    OutWebhook,
    // Observability
    OutPrometheus,
    OutLoki,
    OutElasticsearch,
    OutInfluxdb,
    // Cloud Integrations
    OutGoogleBigquery,
    OutSnowflake,
    OutAwsLambda,
    OutLog,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WadmApplication {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: Spec,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Metadata {
    pub name: String,
    pub annotations: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Spec {
    pub components: Vec<Component>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Component {
    pub name: String,
    #[serde(rename = "type")]
    pub component_type: String,
    pub properties: Properties,
    #[serde(default)]
    pub traits: Vec<Trait>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Properties {
    WithImage {
        image: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        config: Option<Vec<Config>>,
    },
    WithApplication {
        application: ApplicationRef,
    },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicationRef {
    pub name: String,
    pub component: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub name: String,
    pub properties: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Trait {
    #[serde(rename = "type")]
    pub trait_type: String,
    pub properties: TraitProperties,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum TraitProperties {
    Spreadscaler { instances: u32 },
    Link(LinkProperties),
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkProperties {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<LinkSource>,
    pub target: LinkTarget,
    pub namespace: String,
    pub package: String,
    pub interfaces: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkTarget {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    config: Option<Vec<Config>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkSource {
    // #[serde(default, skip_serializing_if = "Option::is_none")]
    // pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config: Vec<Config>,
}

pub fn convert_pipeline(
    pipeline: &Pipeline,
    workspace_slug: &String,
    settings: &Settings,
) -> Result<WadmApplication, Box<dyn std::error::Error>> {
    let mut components = Vec::new();
    let mut step_topics = HashMap::new();

    // Generate topic names for inter-step communication based on dependency depth
    // Build a map of node names to their dependency depth
    let mut node_depths = HashMap::new();

    // Find root nodes (no dependencies)
    for step in &pipeline.nodes {
        if step.depends_on.is_none() || step.depends_on.as_ref().unwrap().is_empty() {
            node_depths.insert(step.name.clone(), 1);
        }
    }

    // Calculate depths for dependent nodes
    let mut changed = true;
    while changed {
        changed = false;
        for step in &pipeline.nodes {
            if let Some(depends_on) = &step.depends_on {
                if !depends_on.is_empty() && !node_depths.contains_key(&step.name) {
                    // Check if all dependencies have been processed
                    let mut max_depth = 0;
                    let mut all_deps_processed = true;
                    for dep in depends_on {
                        if let Some(&depth) = node_depths.get(dep) {
                            max_depth = max_depth.max(depth);
                        } else {
                            all_deps_processed = false;
                            break;
                        }
                    }
                    if all_deps_processed {
                        node_depths.insert(step.name.clone(), max_depth + 1);
                        changed = true;
                    }
                }
            }
        }
    }

    // Generate topics for nodes that have dependencies
    for step in &pipeline.nodes {
        if let Some(depends_on) = &step.depends_on {
            if !depends_on.is_empty() {
                if let Some(&depth) = node_depths.get(&step.name) {
                    let topic = format!("{}-{}-step-{}-in", workspace_slug, pipeline.name, depth);
                    step_topics.insert(step.name.clone(), topic);
                }
            }
        }
    }

    // Process each step
    for step in &pipeline.nodes {
        match step.step_type {
            PipelineNodeType::InHttpWebhook => {
                // Add in-http component
                components.push(Component {
                    name: step.name.clone(),
                    component_type: "component".to_string(),
                    properties: Properties::WithImage {
                        image: format!("{}/pipestack/in-http:0.0.1", settings.registry.url),
                        config: None,
                    },
                    traits: vec![
                        Trait {
                            trait_type: "spreadscaler".to_string(),
                            properties: TraitProperties::Spreadscaler {
                                instances: step.instances.unwrap_or(1),
                            },
                        },
                        Trait {
                            trait_type: "link".to_string(),
                            properties: TraitProperties::Link(LinkProperties {
                                name: None,
                                source: None,
                                target: LinkTarget {
                                    name: format!("out-internal-for-{}", step.name),
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
                let next_topic = pipeline
                    .nodes
                    .iter()
                    .find(|s| {
                        s.depends_on
                            .as_ref()
                            .is_some_and(|deps| deps.contains(&step.name))
                    })
                    .and_then(|s| step_topics.get(&s.name))
                    .cloned()
                    .unwrap_or_default();

                if !next_topic.is_empty() {
                    components.push(Component {
                        name: format!("out-internal-for-{}", step.name),
                        component_type: "component".to_string(),
                        properties: Properties::WithImage {
                            image: format!(
                                "{}/pipestack/out-internal:0.0.1",
                                settings.registry.url
                            ),
                            config: Some(vec![Config {
                                name: format!("out-internal-for-{}-config", step.name),
                                properties: {
                                    let mut props = BTreeMap::new();
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
                        ],
                    });
                }
            }
            PipelineNodeType::ProcessorWasm => {
                // Add in-internal component for processor
                components.push(Component {
                    name: format!("in-internal-for-{}", step.name),
                    component_type: "component".to_string(),
                    properties: Properties::WithImage {
                        image: format!("{}/pipestack/in-internal:0.0.1", settings.registry.url),
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
                                    name: step.name.clone(),
                                    config: None,
                                },
                                namespace: "pipestack".to_string(),
                                package: "customer".to_string(),
                                interfaces: vec!["customer".to_string()],
                            }),
                        },
                        Trait {
                            trait_type: "link".to_string(),
                            properties: TraitProperties::Link(LinkProperties {
                                name: None,
                                source: None,
                                target: LinkTarget {
                                    name: format!("out-internal-for-{}", step.name),
                                    config: None,
                                },
                                namespace: "pipestack".to_string(),
                                package: "out".to_string(),
                                interfaces: vec!["out".to_string()],
                            }),
                        },
                    ],
                });

                // Add the processor component itself
                components.push(Component {
                    name: step.name.clone(),
                    component_type: "component".to_string(),
                    properties: Properties::WithImage {
                        image: format!(
                            "{}/{}/pipeline/{}/{}/builder/components/nodes/processor/wasm/{}:1.0.0",
                            settings.registry.url,
                            workspace_slug,
                            pipeline.name,
                            pipeline.version,
                            step.name
                        ),
                        config: None,
                    },
                    traits: vec![Trait {
                        trait_type: "spreadscaler".to_string(),
                        properties: TraitProperties::Spreadscaler {
                            instances: step.instances.unwrap_or(1),
                        },
                    }],
                });

                // Add out-internal component for processor
                let next_topic = pipeline
                    .nodes
                    .iter()
                    .find(|s| {
                        s.depends_on
                            .as_ref()
                            .is_some_and(|deps| deps.contains(&step.name))
                    })
                    .and_then(|s| step_topics.get(&s.name))
                    .cloned()
                    .unwrap_or_default();

                if !next_topic.is_empty() {
                    components.push(Component {
                        name: format!("out-internal-for-{}", step.name),
                        component_type: "component".to_string(),
                        properties: Properties::WithImage {
                            image: format!(
                                "{}/pipestack/out-internal:0.0.1",
                                settings.registry.url
                            ),
                            config: Some(vec![Config {
                                name: format!("out-internal-for-{}-config", step.name),
                                properties: {
                                    let mut props = BTreeMap::new();
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
                        ],
                    });
                }
            }
            PipelineNodeType::OutLog => {
                // Add in-internal component for out-log
                components.push(Component {
                    name: format!("in-internal-for-{}", step.name),
                    component_type: "component".to_string(),
                    properties: Properties::WithImage {
                        image: format!("{}/pipestack/in-internal:0.0.1", settings.registry.url),
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
                        image: format!("{}/pipestack/out-log:0.0.1", settings.registry.url),
                        config: None,
                    },
                    traits: vec![Trait {
                        trait_type: "spreadscaler".to_string(),
                        properties: TraitProperties::Spreadscaler {
                            instances: step.instances.unwrap_or(1),
                        },
                    }],
                });
            }
            _ => {
                // Handle other step types as needed
            }
        }
    }

    // Add capabilities (httpserver and messaging-nats)
    // HTTP Server capability
    if pipeline
        .nodes
        .iter()
        .any(|s| matches!(s.step_type, PipelineNodeType::InHttpWebhook))
    {
        let http_step = pipeline
            .nodes
            .iter()
            .find(|s| matches!(s.step_type, PipelineNodeType::InHttpWebhook))
            .unwrap();

        components.push(Component {
            name: "httpserver".to_string(),
            component_type: "capability".to_string(),
            properties: Properties::WithApplication {
                application: ApplicationRef {
                    name: format!("{}-providers", workspace_slug),
                    component: "httpserver".to_string(),
                },
            },
            traits: vec![Trait {
                trait_type: "link".to_string(),
                properties: TraitProperties::Link(LinkProperties {
                    name: Some(format!(
                        "httpserver-to-{}-{}-link",
                        workspace_slug, http_step.name
                    )),
                    source: Some(LinkSource {
                        config: vec![Config {
                            name: format!(
                                "{}-{}-httpserver-path-config",
                                workspace_slug, pipeline.name
                            ),
                            properties: {
                                let mut props = BTreeMap::new();
                                props.insert(
                                    "path".to_string(),
                                    serde_yaml::Value::String(format!("/{}", pipeline.name)),
                                );
                                props
                            },
                        }],
                    }),
                    target: LinkTarget {
                        name: http_step.name.clone(),
                        config: None,
                    },
                    namespace: "wasi".to_string(),
                    package: "http".to_string(),
                    interfaces: vec!["incoming-handler".to_string()],
                }),
            }],
        });
    }

    // NATS messaging capability
    let mut nats_traits = vec![];

    // Add messaging-nats links
    let mut subscription_counter = 1;
    for step in &pipeline.nodes {
        if matches!(step.step_type, PipelineNodeType::ProcessorWasm) {
            if let Some(topic) = step_topics.get(&step.name) {
                nats_traits.push(Trait {
                    trait_type: "link".to_string(),
                    properties: TraitProperties::Link(LinkProperties {
                        name: Some(format!(
                            "messaging-nats-to-{}-in-internal-for-{}-link",
                            workspace_slug, step.name
                        )),
                        source: Some(LinkSource {
                            config: vec![Config {
                                name: format!("subscription-{}-config", subscription_counter),
                                properties: {
                                    let mut props = BTreeMap::new();
                                    props.insert(
                                        "subscriptions".to_string(),
                                        serde_yaml::Value::String(topic.clone()),
                                    );
                                    props.insert(
                                        "cluster_uris".to_string(),
                                        serde_yaml::Value::String(
                                            settings.nats.cluster_uris.to_string(),
                                        ),
                                    );
                                    props
                                },
                            }],
                        }),
                        target: LinkTarget {
                            name: format!("in-internal-for-{}", step.name),
                            config: None,
                        },
                        namespace: "wasmcloud".to_string(),
                        package: "messaging".to_string(),
                        interfaces: vec!["handler".to_string()],
                    }),
                });
                subscription_counter += 1;
            }
        }
    }

    for step in &pipeline.nodes {
        if matches!(step.step_type, PipelineNodeType::OutLog) {
            if let Some(topic) = step_topics.get(&step.name) {
                nats_traits.push(Trait {
                    trait_type: "link".to_string(),
                    properties: TraitProperties::Link(LinkProperties {
                        name: Some(format!(
                            "messaging-nats-to-{}-in-internal-for-{}-link",
                            workspace_slug, step.name
                        )),
                        source: Some(LinkSource {
                            config: vec![Config {
                                name: format!("subscription-{}-config", subscription_counter),
                                properties: {
                                    let mut props = BTreeMap::new();
                                    props.insert(
                                        "subscriptions".to_string(),
                                        serde_yaml::Value::String(topic.clone()),
                                    );
                                    props.insert(
                                        "cluster_uris".to_string(),
                                        serde_yaml::Value::String(
                                            settings.nats.cluster_uris.to_string(),
                                        ),
                                    );
                                    props
                                },
                            }],
                        }),
                        target: LinkTarget {
                            name: format!("in-internal-for-{}", step.name),
                            config: None,
                        },
                        namespace: "wasmcloud".to_string(),
                        package: "messaging".to_string(),
                        interfaces: vec!["handler".to_string()],
                    }),
                });
                subscription_counter += 1;
            }
        }
    }

    components.push(Component {
        name: "messaging-nats".to_string(),
        component_type: "capability".to_string(),
        properties: Properties::WithApplication {
            application: ApplicationRef {
                name: format!("{}-providers", workspace_slug),
                component: "messaging-nats".to_string(),
            },
        },
        traits: nats_traits,
    });

    Ok(WadmApplication {
        api_version: "core.oam.dev/v1beta1".to_string(),
        kind: "Application".to_string(),
        metadata: Metadata {
            name: format!("{}-{}", workspace_slug, pipeline.name,),
            annotations: {
                let mut annotations = BTreeMap::new();
                annotations.insert("version".to_string(), pipeline.version.clone());
                annotations
            },
        },
        spec: Spec { components },
    })
}

pub fn create_providers_wadm(workspace_slug: &str, settings: &Settings) -> WadmApplication {
    let mut annotations = BTreeMap::new();
    annotations.insert(
        "experimental.wasmcloud.dev/shared".to_string(),
        "true".to_string(),
    );
    annotations.insert(
        "description".to_string(),
        format!("Shared providers for the {} workspace", workspace_slug),
    );
    annotations.insert("version".to_string(), "0.1.0".to_string());

    let metadata = Metadata {
        name: format!("{}-providers", workspace_slug),
        annotations,
    };

    // HTTP Server component
    let mut http_config_props = BTreeMap::new();
    http_config_props.insert(
        "routing_mode".to_string(),
        serde_yaml::Value::String("path".to_string()),
    );
    http_config_props.insert(
        "address".to_string(),
        serde_yaml::Value::String("0.0.0.0:8000".to_string()),
    );

    let http_config = Config {
        name: "default-http-config".to_string(),
        properties: http_config_props,
    };

    let http_properties = Properties::WithImage {
        image: "ghcr.io/wasmcloud/http-server:0.27.0".to_string(),
        config: Some(vec![http_config]),
    };

    let http_trait = Trait {
        trait_type: "spreadscaler".to_string(),
        properties: TraitProperties::Spreadscaler { instances: 1 },
    };

    let http_component = Component {
        name: "httpserver".to_string(),
        component_type: "capability".to_string(),
        properties: http_properties,
        traits: vec![http_trait],
    };

    // Messaging NATS component
    let nats_properties = Properties::WithImage {
        image: "ghcr.io/wasmcloud/messaging-nats:0.27.0".to_string(),
        config: Some(vec![Config {
            name: "messaging-nats-config".to_string(),
            properties: {
                let mut props = BTreeMap::new();
                props.insert(
                    "cluster_uris".to_string(),
                    serde_yaml::Value::String(settings.nats.cluster_uris.to_string()),
                );
                props
            },
        }]),
    };

    let nats_trait = Trait {
        trait_type: "spreadscaler".to_string(),
        properties: TraitProperties::Spreadscaler { instances: 1 },
    };

    let nats_component = Component {
        name: "messaging-nats".to_string(),
        component_type: "capability".to_string(),
        properties: nats_properties,
        traits: vec![nats_trait],
    };

    let spec = Spec {
        components: vec![http_component, nats_component],
    };

    WadmApplication {
        api_version: "core.oam.dev/v1beta1".to_string(),
        kind: "Application".to_string(),
        metadata,
        spec,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_pipeline_in_processor_out() {
        let input_yaml = r#"
name: mine
version: 1
nodes:
  - name: in-http-webhook_17
    type: in-http-webhook
    position:
      x: 300
      'y': 180
  - name: processor-wasm_18
    type: processor-wasm
    position:
      x: 548
      'y': 69
    source: localhost:5000/pipestack/data-processor:0.0.1
    instances: 1
    depends_on:
      - in-http-webhook_17
  - name: out-log_19
    type: out-log
    position:
      x: 660
      'y': 180
    depends_on:
      - processor-wasm_18
"#;

        let expected_yaml = r#"apiVersion: core.oam.dev/v1beta1
kind: Application
metadata:
  name: default-mine
  annotations:
    version: '1'
spec:
  components:
  - name: in-http-webhook_17
    type: component
    properties:
      image: http://localhost:5000/pipestack/in-http:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        namespace: pipestack
        package: out
        interfaces:
        - out
        target:
          name: out-internal-for-in-http-webhook_17
  - name: out-internal-for-in-http-webhook_17
    type: component
    properties:
      image: http://localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-in-http-webhook_17-config
        properties:
          next-step-topic: default-mine-step-2-in
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
        target:
          name: messaging-nats
  - name: in-internal-for-processor-wasm_18
    type: component
    properties:
      image: http://localhost:5000/pipestack/in-internal:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        namespace: pipestack
        package: customer
        interfaces:
        - customer
        target:
          name: processor-wasm_18
    - type: link
      properties:
        namespace: pipestack
        package: out
        interfaces:
        - out
        target:
          name: out-internal-for-processor-wasm_18
  - name: processor-wasm_18
    type: component
    properties:
      image: http://localhost:5000/default/pipeline/mine/1/builder/components/nodes/processor/wasm/processor-wasm_18:1.0.0
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: out-internal-for-processor-wasm_18
    type: component
    properties:
      image: http://localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-processor-wasm_18-config
        properties:
          next-step-topic: default-mine-step-3-in
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
        target:
          name: messaging-nats
  - name: in-internal-for-out-log_19
    type: component
    properties:
      image: http://localhost:5000/pipestack/in-internal:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
        target:
          name: messaging-nats
    - type: link
      properties:
        namespace: pipestack
        package: out
        interfaces:
        - out
        target:
          name: out-log_19
  - name: out-log_19
    type: component
    properties:
      image: http://localhost:5000/pipestack/out-log:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: httpserver
    type: capability
    properties:
      application:
        name: default-providers
        component: httpserver
    traits:
    - type: link
      properties:
        namespace: wasi
        package: http
        interfaces:
        - incoming-handler
        source:
          config:
          - name: default-mine-httpserver-path-config
            properties:
              path: /mine
        target:
          name: in-http-webhook_17
        name: httpserver-to-default-in-http-webhook_17-link
  - name: messaging-nats
    type: capability
    properties:
      application:
        name: default-providers
        component: messaging-nats
    traits:
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
        source:
          config:
          - name: subscription-1-config
            properties:
              subscriptions: default-mine-step-2-in
              cluster_uris: localhost:4222
        target:
          name: in-internal-for-processor-wasm_18
        name: messaging-nats-to-default-in-internal-for-processor-wasm_18-link
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
        source:
          config:
          - name: subscription-2-config
            properties:
              subscriptions: default-mine-step-3-in
              cluster_uris: localhost:4222
        target:
          name: in-internal-for-out-log_19
        name: messaging-nats-to-default-in-internal-for-out-log_19-link
"#;

        let settings = Settings::new().expect("Could not read config settings");

        // Parse input
        let pipeline: Pipeline =
            serde_yaml::from_str(input_yaml).expect("Failed to parse input YAML");

        // Convert to WADM
        let actual_wadm = convert_pipeline(&pipeline, &"default".to_string(), &settings)
            .expect("Failed to convert pipeline");

        // Parse expected output to same struct type
        let expected_wadm: WadmApplication =
            serde_yaml::from_str(expected_yaml).expect("Failed to parse expected YAML");

        // STRUCTURED COMPARISON - much more reliable!
        assert_eq!(actual_wadm, expected_wadm);
    }

    #[test]
    fn test_convert_pipeline_in_processor_out_out() {
        let input_yaml = r#"
name: mine
version: 1
nodes:
  - name: in-http-webhook_17
    type: in-http-webhook
    position:
      x: 300
      'y': 180
  - name: processor-wasm_18
    type: processor-wasm
    position:
      x: 548
      'y': 69
    source: localhost:5000/pipestack/data-processor:0.0.1
    instances: 1
    depends_on:
      - in-http-webhook_17
  - name: out-log_19
    type: out-log
    position:
      x: 660
      'y': 180
    depends_on:
      - processor-wasm_18
  - name: out-log_20
    type: out-log
    position:
      x: 960
      'y': 180
    depends_on:
      - processor-wasm_18
"#;

        let expected_yaml = r#"apiVersion: core.oam.dev/v1beta1
kind: Application
metadata:
  name: default-mine
  annotations:
    version: '1'
spec:
  components:
  - name: in-http-webhook_17
    type: component
    properties:
      image: http://localhost:5000/pipestack/in-http:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        namespace: pipestack
        package: out
        interfaces:
        - out
        target:
          name: out-internal-for-in-http-webhook_17
  - name: out-internal-for-in-http-webhook_17
    type: component
    properties:
      image: http://localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-in-http-webhook_17-config
        properties:
          next-step-topic: default-mine-step-2-in
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
        target:
          name: messaging-nats
  - name: in-internal-for-processor-wasm_18
    type: component
    properties:
      image: http://localhost:5000/pipestack/in-internal:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        namespace: pipestack
        package: customer
        interfaces:
        - customer
        target:
          name: processor-wasm_18
    - type: link
      properties:
        namespace: pipestack
        package: out
        interfaces:
        - out
        target:
          name: out-internal-for-processor-wasm_18
  - name: processor-wasm_18
    type: component
    properties:
      image: http://localhost:5000/default/pipeline/mine/1/builder/components/nodes/processor/wasm/processor-wasm_18:1.0.0
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: out-internal-for-processor-wasm_18
    type: component
    properties:
      image: http://localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-processor-wasm_18-config
        properties:
          next-step-topic: default-mine-step-3-in
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
        target:
          name: messaging-nats
  - name: in-internal-for-out-log_19
    type: component
    properties:
      image: http://localhost:5000/pipestack/in-internal:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
        target:
          name: messaging-nats
    - type: link
      properties:
        namespace: pipestack
        package: out
        interfaces:
        - out
        target:
          name: out-log_19
  - name: out-log_19
    type: component
    properties:
      image: http://localhost:5000/pipestack/out-log:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: in-internal-for-out-log_20
    type: component
    properties:
      image: http://localhost:5000/pipestack/in-internal:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
        target:
          name: messaging-nats
    - type: link
      properties:
        namespace: pipestack
        package: out
        interfaces:
        - out
        target:
          name: out-log_20
  - name: out-log_20
    type: component
    properties:
      image: http://localhost:5000/pipestack/out-log:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: httpserver
    type: capability
    properties:
      application:
        name: default-providers
        component: httpserver
    traits:
    - type: link
      properties:
        namespace: wasi
        package: http
        interfaces:
        - incoming-handler
        source:
          config:
          - name: default-mine-httpserver-path-config
            properties:
              path: /mine
        target:
          name: in-http-webhook_17
        name: httpserver-to-default-in-http-webhook_17-link
  - name: messaging-nats
    type: capability
    properties:
      application:
        name: default-providers
        component: messaging-nats
    traits:
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
        source:
          config:
          - name: subscription-1-config
            properties:
              subscriptions: default-mine-step-2-in
              cluster_uris: localhost:4222
        target:
          name: in-internal-for-processor-wasm_18
        name: messaging-nats-to-default-in-internal-for-processor-wasm_18-link
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
        source:
          config:
          - name: subscription-2-config
            properties:
              subscriptions: default-mine-step-3-in
              cluster_uris: localhost:4222
        target:
          name: in-internal-for-out-log_19
        name: messaging-nats-to-default-in-internal-for-out-log_19-link
    - type: link
      properties:
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
        source:
          config:
          - name: subscription-3-config
            properties:
              subscriptions: default-mine-step-3-in
              cluster_uris: localhost:4222
        target:
          name: in-internal-for-out-log_20
        name: messaging-nats-to-default-in-internal-for-out-log_20-link
"#;

        let settings = Settings::new().expect("Could not read config settings");

        // Parse input
        let pipeline: Pipeline =
            serde_yaml::from_str(input_yaml).expect("Failed to parse input YAML");

        // Convert to WADM
        let actual_wadm = convert_pipeline(&pipeline, &"default".to_string(), &settings)
            .expect("Failed to convert pipeline");

        // Parse expected output to same struct type
        let expected_wadm: WadmApplication =
            serde_yaml::from_str(expected_yaml).expect("Failed to parse expected YAML");

        // STRUCTURED COMPARISON - much more reliable!
        assert_eq!(actual_wadm, expected_wadm);
    }
}
