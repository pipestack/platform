use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use ts_rs::TS;

const PIPELINE_TS_FILE_PATH: &str = "./pipeline.ts";
const REGISTRY_URL: &'static str = match std::option_env!("REGISTRY_URL") {
    Some(url) => url,
    None => "localhost:5000",
};
const NATS_CLUSTER_URIS: &'static str = match std::option_env!("NATS_CLUSTER_URIS") {
    Some(uris) => uris,
    None => "localhost:4222",
};

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
    pub source: Option<String>,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct WadmApplication {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: Spec,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub name: String,
    pub annotations: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Spec {
    pub components: Vec<Component>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    #[serde(rename = "type")]
    pub component_type: String,
    pub properties: Properties,
    #[serde(default)]
    pub traits: Vec<Trait>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Properties {
    pub image: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config: Vec<Config>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub name: String,
    pub properties: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Trait {
    #[serde(rename = "type")]
    pub trait_type: String,
    pub properties: TraitProperties,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TraitProperties {
    Spreadscaler { instances: u32 },
    Link(LinkProperties),
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LinkTarget {
    Name {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        config: Option<Vec<Config>>,
    },
    String(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LinkSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config: Vec<Config>,
}

pub fn convert_pipeline(
    pipeline: &Pipeline,
    workspace_slug: &String,
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
                    properties: Properties {
                        image: format!("{}/pipestack/in-http:0.0.1", REGISTRY_URL),
                        config: vec![],
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
                                target: LinkTarget::Name {
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
                            .map_or(false, |deps| deps.contains(&step.name))
                    })
                    .and_then(|s| step_topics.get(&s.name))
                    .cloned()
                    .unwrap_or_default();

                if !next_topic.is_empty() {
                    components.push(Component {
                        name: format!("out-internal-for-{}", step.name),
                        component_type: "component".to_string(),
                        properties: Properties {
                            image: format!("{}/pipestack/out-internal:0.0.1", REGISTRY_URL),
                            config: vec![Config {
                                name: format!("out-internal-for-{}-config", step.name),
                                properties: {
                                    let mut props = BTreeMap::new();
                                    props.insert(
                                        "next-step-topic".to_string(),
                                        serde_yaml::Value::String(next_topic.clone()),
                                    );
                                    props
                                },
                            }],
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
                                    target: LinkTarget::Name {
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
                    properties: Properties {
                        image: format!("{}/pipestack/in-internal:0.0.1", REGISTRY_URL),
                        config: vec![],
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
                                target: LinkTarget::String(step.name.clone()),
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
                                target: LinkTarget::String(format!(
                                    "out-internal-for-{}",
                                    step.name
                                )),
                                namespace: "pipestack".to_string(),
                                package: "out".to_string(),
                                interfaces: vec!["out".to_string()],
                            }),
                        },
                    ],
                });

                // Add the processor component itself
                let image = if let Some(source) = &step.source {
                    source.clone()
                } else {
                    format!("{}/pipestack/{}:0.0.1", REGISTRY_URL, step.name)
                };

                components.push(Component {
                    name: step.name.clone(),
                    component_type: "component".to_string(),
                    properties: Properties {
                        image,
                        config: vec![],
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
                            .map_or(false, |deps| deps.contains(&step.name))
                    })
                    .and_then(|s| step_topics.get(&s.name))
                    .cloned()
                    .unwrap_or_default();

                if !next_topic.is_empty() {
                    components.push(Component {
                        name: format!("out-internal-for-{}", step.name),
                        component_type: "component".to_string(),
                        properties: Properties {
                            image: format!("{}/pipestack/out-internal:0.0.1", REGISTRY_URL),
                            config: vec![Config {
                                name: format!("out-internal-for-{}-config", step.name),
                                properties: {
                                    let mut props = BTreeMap::new();
                                    props.insert(
                                        "next-step-topic".to_string(),
                                        serde_yaml::Value::String(next_topic.clone()),
                                    );
                                    props
                                },
                            }],
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
                                    target: LinkTarget::Name {
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
                    properties: Properties {
                        image: format!("{}/pipestack/in-internal:0.0.1", REGISTRY_URL),
                        config: vec![],
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
                                target: LinkTarget::Name {
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
                                target: LinkTarget::String(step.name.clone()),
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
                    properties: Properties {
                        image: format!("{}/pipestack/out-log:0.0.1", REGISTRY_URL),
                        config: vec![],
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
            properties: Properties {
                image: "ghcr.io/wasmcloud/http-server:0.27.0".to_string(),
                config: vec![Config {
                    name: "default-http-config".to_string(),
                    properties: {
                        let mut props = BTreeMap::new();
                        props.insert(
                            "routing_mode".to_string(),
                            serde_yaml::Value::String("path".to_string()),
                        );
                        props.insert(
                            "address".to_string(),
                            serde_yaml::Value::String("0.0.0.0:8000".to_string()),
                        );
                        props
                    },
                }],
            },
            traits: vec![
                Trait {
                    trait_type: "spreadscaler".to_string(),
                    properties: TraitProperties::Spreadscaler { instances: 1 },
                },
                Trait {
                    trait_type: "link".to_string(),
                    properties: TraitProperties::Link(LinkProperties {
                        name: Some(format!("httpserver-to-{}-link", http_step.name)),
                        source: Some(LinkSource {
                            name: None,
                            config: vec![Config {
                                name: "path-config".to_string(),
                                properties: {
                                    let mut props = BTreeMap::new();
                                    props.insert(
                                        "path".to_string(),
                                        serde_yaml::Value::String(format!(
                                            "/{}-{}",
                                            workspace_slug, pipeline.name
                                        )),
                                    );
                                    props
                                },
                            }],
                        }),
                        target: LinkTarget::Name {
                            name: http_step.name.clone(),
                            config: None,
                        },
                        namespace: "wasi".to_string(),
                        package: "http".to_string(),
                        interfaces: vec!["incoming-handler".to_string()],
                    }),
                },
            ],
        });
    }

    // NATS messaging capability
    let mut nats_traits = vec![Trait {
        trait_type: "spreadscaler".to_string(),
        properties: TraitProperties::Spreadscaler { instances: 1 },
    }];

    // Add messaging-nats links
    let mut subscription_counter = 1;
    for step in &pipeline.nodes {
        if matches!(step.step_type, PipelineNodeType::ProcessorWasm) {
            if let Some(topic) = step_topics.get(&step.name) {
                nats_traits.push(Trait {
                    trait_type: "link".to_string(),
                    properties: TraitProperties::Link(LinkProperties {
                        name: Some(format!(
                            "messaging-nats-to-in-internal-for-{}-link",
                            step.name
                        )),
                        source: Some(LinkSource {
                            name: Some("messaging-nats".to_string()),
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
                                            NATS_CLUSTER_URIS.to_string(),
                                        ),
                                    );
                                    props
                                },
                            }],
                        }),
                        target: LinkTarget::Name {
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
                            "messaging-nats-to-in-internal-for-{}-link",
                            step.name
                        )),
                        source: Some(LinkSource {
                            name: Some("messaging-nats".to_string()),
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
                                            NATS_CLUSTER_URIS.to_string(),
                                        ),
                                    );
                                    props
                                },
                            }],
                        }),
                        target: LinkTarget::Name {
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
        properties: Properties {
            image: "ghcr.io/wasmcloud/messaging-nats:0.27.0".to_string(),
            config: vec![Config {
                name: "messaging-nats-config".to_string(),
                properties: {
                    let mut props = BTreeMap::new();
                    props.insert(
                        "cluster_uris".to_string(),
                        serde_yaml::Value::String(NATS_CLUSTER_URIS.to_string()),
                    );
                    props
                },
            }],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_pipeline_2() {
        let input_yaml = r#"
name: untitled-pipeline
version: 1.0.0
nodes:
  - name: in-http-webhook_http_1_1750048123367
    type: in-http-webhook
    position:
      x: 100.0
      y: 100.0
  - name: processor_wasm_2_1750048126167
    type: processor-wasm
    source: file:///path/to/data-processor.wasm
    instances: 1
    position:
      x: 200.0
      y: 100.0
    depends_on:
      - in-http-webhook_http_1_1750048123367
  - name: out-log_log_3_1750048128320
    type: out-log
    position:
      x: 300.0
      y: 100.0
    depends_on:
      - processor_wasm_2_1750048126167
"#;

        let expected_yaml = r#"apiVersion: core.oam.dev/v1beta1
kind: Application
metadata:
  name: test-workspace-untitled-pipeline
  annotations:
    version: 1.0.0
spec:
  components:
  - name: in-http-webhook_http_1_1750048123367
    type: component
    properties:
      image: localhost:5000/pipestack/in-http:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target:
          name: out-internal-for-in-http-webhook_http_1_1750048123367
        namespace: pipestack
        package: out
        interfaces:
        - out
  - name: out-internal-for-in-http-webhook_http_1_1750048123367
    type: component
    properties:
      image: localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-in-http-webhook_http_1_1750048123367-config
        properties:
          next-step-topic: test-workspace-untitled-pipeline-step-2-in
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target:
          name: messaging-nats
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
  - name: in-internal-for-processor_wasm_2_1750048126167
    type: component
    properties:
      image: localhost:5000/pipestack/in-internal:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target: processor_wasm_2_1750048126167
        namespace: pipestack
        package: customer
        interfaces:
        - customer
    - type: link
      properties:
        target: out-internal-for-processor_wasm_2_1750048126167
        namespace: pipestack
        package: out
        interfaces:
        - out
  - name: processor_wasm_2_1750048126167
    type: component
    properties:
      image: file:///path/to/data-processor.wasm
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: out-internal-for-processor_wasm_2_1750048126167
    type: component
    properties:
      image: localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-processor_wasm_2_1750048126167-config
        properties:
          next-step-topic: test-workspace-untitled-pipeline-step-3-in
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target:
          name: messaging-nats
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
  - name: in-internal-for-out-log_log_3_1750048128320
    type: component
    properties:
      image: localhost:5000/pipestack/in-internal:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target:
          name: messaging-nats
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
    - type: link
      properties:
        target: out-log_log_3_1750048128320
        namespace: pipestack
        package: out
        interfaces:
        - out
  - name: out-log_log_3_1750048128320
    type: component
    properties:
      image: localhost:5000/pipestack/out-log:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: httpserver
    type: capability
    properties:
      image: ghcr.io/wasmcloud/http-server:0.27.0
      config:
      - name: default-http-config
        properties:
          address: 0.0.0.0:8000
          routing_mode: path
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        name: httpserver-to-in-http-webhook_http_1_1750048123367-link
        source:
          config:
          - name: path-config
            properties:
              path: /test-workspace-untitled-pipeline
        target:
          name: in-http-webhook_http_1_1750048123367
        namespace: wasi
        package: http
        interfaces:
        - incoming-handler
  - name: messaging-nats
    type: capability
    properties:
      image: ghcr.io/wasmcloud/messaging-nats:0.27.0
      config:
      - name: messaging-nats-config
        properties:
          cluster_uris: nats://nats.railway.internal:4222
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        name: messaging-nats-to-in-internal-for-processor_wasm_2_1750048126167-link
        source:
          name: messaging-nats
          config:
          - name: subscription-1-config
            properties:
              cluster_uris: nats://nats.railway.internal:4222
              subscriptions: test-workspace-untitled-pipeline-step-2-in
        target:
          name: in-internal-for-processor_wasm_2_1750048126167
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
    - type: link
      properties:
        name: messaging-nats-to-in-internal-for-out-log_log_3_1750048128320-link
        source:
          name: messaging-nats
          config:
          - name: subscription-2-config
            properties:
              cluster_uris: nats://nats.railway.internal:4222
              subscriptions: test-workspace-untitled-pipeline-step-3-in
        target:
          name: in-internal-for-out-log_log_3_1750048128320
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler"#;

        // Parse the input YAML into a Pipeline struct
        let pipeline: Pipeline =
            serde_yaml::from_str(input_yaml).expect("Failed to parse input YAML");

        // Convert the pipeline to WadmApplication
        let wadm_app = convert_pipeline(&pipeline, &String::from("test-workspace"))
            .expect("Failed to convert pipeline");

        // Convert back to YAML
        let output_yaml =
            serde_yaml::to_string(&wadm_app).expect("Failed to serialize WadmApplication to YAML");

        // Compare with expected output
        assert_eq!(output_yaml.trim(), expected_yaml.trim());
    }

    #[test]
    fn test_convert_pipeline_3() {
        let input_yaml = r#"
name: untitled-pipeline
version: 1.0.0
nodes:
  - name: in-http-webhook_http_1_1750048123367
    type: in-http-webhook
    position:
      x: 100.0
      y: 100.0
  - name: processor_wasm_2_1750048126167
    type: processor-wasm
    source: file:///path/to/data-processor.wasm
    instances: 1
    position:
      x: 200.0
      y: 100.0
    depends_on:
      - in-http-webhook_http_1_1750048123367
  - name: out-log_log_3_1750048128320
    type: out-log
    position:
      x: 300.0
      y: 100.0
    depends_on:
      - processor_wasm_2_1750048126167
  - name: out-log_log_4_1750048128320
    type: out-log
    position:
      x: 300.0
      y: 100.0
    depends_on:
      - processor_wasm_2_1750048126167
"#;

        let expected_yaml = r#"apiVersion: core.oam.dev/v1beta1
kind: Application
metadata:
  name: test-workspace-untitled-pipeline
  annotations:
    version: 1.0.0
spec:
  components:
  - name: in-http-webhook_http_1_1750048123367
    type: component
    properties:
      image: localhost:5000/pipestack/in-http:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target:
          name: out-internal-for-in-http-webhook_http_1_1750048123367
        namespace: pipestack
        package: out
        interfaces:
        - out
  - name: out-internal-for-in-http-webhook_http_1_1750048123367
    type: component
    properties:
      image: localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-in-http-webhook_http_1_1750048123367-config
        properties:
          next-step-topic: test-workspace-untitled-pipeline-step-2-in
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target:
          name: messaging-nats
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
  - name: in-internal-for-processor_wasm_2_1750048126167
    type: component
    properties:
      image: localhost:5000/pipestack/in-internal:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target: processor_wasm_2_1750048126167
        namespace: pipestack
        package: customer
        interfaces:
        - customer
    - type: link
      properties:
        target: out-internal-for-processor_wasm_2_1750048126167
        namespace: pipestack
        package: out
        interfaces:
        - out
  - name: processor_wasm_2_1750048126167
    type: component
    properties:
      image: file:///path/to/data-processor.wasm
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: out-internal-for-processor_wasm_2_1750048126167
    type: component
    properties:
      image: localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-processor_wasm_2_1750048126167-config
        properties:
          next-step-topic: test-workspace-untitled-pipeline-step-3-in
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target:
          name: messaging-nats
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
  - name: in-internal-for-out-log_log_3_1750048128320
    type: component
    properties:
      image: localhost:5000/pipestack/in-internal:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target:
          name: messaging-nats
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
    - type: link
      properties:
        target: out-log_log_3_1750048128320
        namespace: pipestack
        package: out
        interfaces:
        - out
  - name: out-log_log_3_1750048128320
    type: component
    properties:
      image: localhost:5000/pipestack/out-log:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: in-internal-for-out-log_log_4_1750048128320
    type: component
    properties:
      image: localhost:5000/pipestack/in-internal:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target:
          name: messaging-nats
        namespace: wasmcloud
        package: messaging
        interfaces:
        - consumer
    - type: link
      properties:
        target: out-log_log_4_1750048128320
        namespace: pipestack
        package: out
        interfaces:
        - out
  - name: out-log_log_4_1750048128320
    type: component
    properties:
      image: localhost:5000/pipestack/out-log:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: httpserver
    type: capability
    properties:
      image: ghcr.io/wasmcloud/http-server:0.27.0
      config:
      - name: default-http-config
        properties:
          address: 0.0.0.0:8000
          routing_mode: path
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        name: httpserver-to-in-http-webhook_http_1_1750048123367-link
        source:
          config:
          - name: path-config
            properties:
              path: /test-workspace-untitled-pipeline
        target:
          name: in-http-webhook_http_1_1750048123367
        namespace: wasi
        package: http
        interfaces:
        - incoming-handler
  - name: messaging-nats
    type: capability
    properties:
      image: ghcr.io/wasmcloud/messaging-nats:0.27.0
      config:
      - name: messaging-nats-config
        properties:
          cluster_uris: nats://nats.railway.internal:4222
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        name: messaging-nats-to-in-internal-for-processor_wasm_2_1750048126167-link
        source:
          name: messaging-nats
          config:
          - name: subscription-1-config
            properties:
              cluster_uris: nats://nats.railway.internal:4222
              subscriptions: test-workspace-untitled-pipeline-step-2-in
        target:
          name: in-internal-for-processor_wasm_2_1750048126167
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
    - type: link
      properties:
        name: messaging-nats-to-in-internal-for-out-log_log_3_1750048128320-link
        source:
          name: messaging-nats
          config:
          - name: subscription-2-config
            properties:
              cluster_uris: nats://nats.railway.internal:4222
              subscriptions: test-workspace-untitled-pipeline-step-3-in
        target:
          name: in-internal-for-out-log_log_3_1750048128320
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
    - type: link
      properties:
        name: messaging-nats-to-in-internal-for-out-log_log_4_1750048128320-link
        source:
          name: messaging-nats
          config:
          - name: subscription-3-config
            properties:
              cluster_uris: nats://nats.railway.internal:4222
              subscriptions: test-workspace-untitled-pipeline-step-3-in
        target:
          name: in-internal-for-out-log_log_4_1750048128320
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler"#;

        // Parse the input YAML into a Pipeline struct
        let pipeline: Pipeline =
            serde_yaml::from_str(input_yaml).expect("Failed to parse input YAML");

        // Convert the pipeline to WadmApplication
        let wadm_app = convert_pipeline(&pipeline, &String::from("test-workspace"))
            .expect("Failed to convert pipeline");

        // Convert back to YAML
        let output_yaml =
            serde_yaml::to_string(&wadm_app).expect("Failed to serialize WadmApplication to YAML");

        // Compare with expected output
        assert_eq!(output_yaml.trim(), expected_yaml.trim());
    }
}
