use serde::{Deserialize, Serialize};
use std::collections::{HashMap, BTreeMap};

#[derive(Debug, Deserialize, Serialize)]
pub struct Pipeline {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub steps: Vec<PipelineStep>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PipelineStep {
    pub name: String,
    #[serde(rename = "type")]
    pub step_type: PipelineStepType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instances: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depends_on: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PipelineStepType {
    InHttp,
    InKafka,
    InS3,
    InPostgres,
    InRedis,
    InFile,
    InGrpc,
    Processor,
    OutKafka,
    OutS3,
    OutPostgres,
    OutRedis,
    OutHttp,
    OutFile,
    OutElasticsearch,
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
    pub target: LinkTarget,
    pub namespace: String,
    pub package: String,
    pub interfaces: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<LinkSource>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LinkTarget {
    Name { name: String },
    String(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LinkSource {
    pub config: Vec<Config>,
}

pub fn convert_pipeline(pipeline: &Pipeline) -> Result<WadmApplication, Box<dyn std::error::Error>> {
    let mut components = Vec::new();
    let mut step_topics = HashMap::new();
    
    // Generate topic names for inter-step communication
    // Group steps by their dependencies to assign the same topic to steps with same dependencies
    let mut dependency_groups: HashMap<String, String> = HashMap::new();
    let mut topic_counter = 2; // Start from step 2 since step 1 is the input
    
    for step in &pipeline.steps {
        if let Some(depends_on) = &step.depends_on {
            if !depends_on.is_empty() {
                let dependency_key = depends_on.join(",");
                if !dependency_groups.contains_key(&dependency_key) {
                    dependency_groups.insert(dependency_key.clone(), format!("{}-step-{}-in", pipeline.name, topic_counter));
                    topic_counter += 1;
                }
                step_topics.insert(step.name.clone(), dependency_groups[&dependency_key].clone());
            }
        }
    }
    
    // Process each step
    for step in &pipeline.steps {
        match step.step_type {
            PipelineStepType::InHttp => {
                // Add in-http component
                components.push(Component {
                    name: step.name.clone(),
                    component_type: "component".to_string(),
                    properties: Properties {
                        image: format!("localhost:5000/pipestack/in-http:0.0.1"),
                        config: vec![],
                    },
                    traits: vec![
                        Trait {
                            trait_type: "spreadscaler".to_string(),
                            properties: TraitProperties::Spreadscaler { 
                                instances: step.instances.unwrap_or(1) 
                            },
                        },
                        Trait {
                            trait_type: "link".to_string(),
                            properties: TraitProperties::Link(LinkProperties {
                                name: None,
                                target: LinkTarget::String(format!("out-internal-for-{}", step.name)),
                                namespace: "pipestack".to_string(),
                                package: "out".to_string(),
                                interfaces: vec!["out".to_string()],
                                source: None,
                            }),
                        },
                    ],
                });
                
                // Add corresponding out-internal component
                let next_topic = if let Some(depends_on) = step.depends_on.as_ref() {
                    depends_on.iter()
                        .filter_map(|dep| step_topics.get(dep))
                        .next()
                        .cloned()
                        .unwrap_or_default()
                } else {
                    // Find steps that depend on this one
                    pipeline.steps.iter()
                        .find(|s| s.depends_on.as_ref().map_or(false, |deps| deps.contains(&step.name)))
                        .and_then(|s| step_topics.get(&s.name))
                        .cloned()
                        .unwrap_or_default()
                };
                
                if !next_topic.is_empty() {
                    components.push(Component {
                        name: format!("out-internal-for-{}", step.name),
                        component_type: "component".to_string(),
                        properties: Properties {
                            image: "localhost:5000/pipestack/out-internal:0.0.1".to_string(),
                            config: vec![Config {
                                name: format!("out-internal-for-{}-config", step.name),
                                properties: {
                                    let mut props = BTreeMap::new();
                                    props.insert("next-step-topic".to_string(), serde_yaml::Value::String(next_topic));
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
                                    target: LinkTarget::Name { name: "messaging-nats".to_string() },
                                    namespace: "wasmcloud".to_string(),
                                    package: "messaging".to_string(),
                                    interfaces: vec!["consumer".to_string()],
                                    source: None,
                                }),
                            },
                        ],
                    });
                }
            },
            PipelineStepType::Processor => {
                // Add in-internal component
                let next_topics: Vec<String> = pipeline.steps.iter()
                    .filter(|s| s.depends_on.as_ref().map_or(false, |deps| deps.contains(&step.name)))
                    .filter_map(|s| step_topics.get(&s.name))
                    .cloned()
                    .collect();
                
                let config = if !next_topics.is_empty() {
                    vec![Config {
                        name: format!("{}-config", step.name),
                        properties: {
                            let mut props = BTreeMap::new();
                            props.insert("next-step-topic".to_string(), 
                                serde_yaml::Value::String(next_topics[0].clone()));
                            props
                        },
                    }]
                } else {
                    vec![]
                };
                
                components.push(Component {
                    name: format!("in-internal-for-{}", step.name),
                    component_type: "component".to_string(),
                    properties: Properties {
                        image: "localhost:5000/pipestack/in-internal:0.0.1".to_string(),
                        config,
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
                                target: LinkTarget::String(step.name.clone()),
                                namespace: "pipestack".to_string(),
                                package: "customer".to_string(),
                                interfaces: vec!["customer".to_string()],
                                source: None,
                            }),
                        },
                        Trait {
                            trait_type: "link".to_string(),
                            properties: TraitProperties::Link(LinkProperties {
                                name: None,
                                target: LinkTarget::String(format!("out-internal-for-{}", step.name)),
                                namespace: "pipestack".to_string(),
                                package: "out".to_string(),
                                interfaces: vec!["out".to_string()],
                                source: None,
                            }),
                        },
                    ],
                });
                
                // Add the processor component itself
                let image = if let Some(source) = &step.source {
                    source.clone()
                } else {
                    format!("localhost:5000/pipestack/{}:0.0.1", step.name)
                };
                
                components.push(Component {
                    name: step.name.clone(),
                    component_type: "component".to_string(),
                    properties: Properties {
                        image,
                        config: vec![],
                    },
                    traits: vec![
                        Trait {
                            trait_type: "spreadscaler".to_string(),
                            properties: TraitProperties::Spreadscaler { 
                                instances: step.instances.unwrap_or(1) 
                            },
                        },
                    ],
                });
                
                // Add out-internal component for processor
                let next_topics: Vec<String> = pipeline.steps.iter()
                    .filter(|s| s.depends_on.as_ref().map_or(false, |deps| deps.contains(&step.name)))
                    .filter_map(|s| step_topics.get(&s.name))
                    .cloned()
                    .collect();
                
                if !next_topics.is_empty() {
                    components.push(Component {
                        name: format!("out-internal-for-{}", step.name),
                        component_type: "component".to_string(),
                        properties: Properties {
                            image: "localhost:5000/pipestack/out-internal:0.0.1".to_string(),
                            config: vec![Config {
                                name: format!("out-internal-for-{}-config", step.name),
                                properties: {
                                    let mut props = BTreeMap::new();
                                    props.insert("next-step-topic".to_string(), 
                                        serde_yaml::Value::String(next_topics[0].clone()));
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
                                    target: LinkTarget::Name { name: "messaging-nats".to_string() },
                                    namespace: "wasmcloud".to_string(),
                                    package: "messaging".to_string(),
                                    interfaces: vec!["consumer".to_string()],
                                    source: None,
                                }),
                            },
                        ],
                    });
                }
            },
            PipelineStepType::OutLog => {
                // Add in-internal component for out-log
                components.push(Component {
                    name: format!("in-internal-for-{}", step.name),
                    component_type: "component".to_string(),
                    properties: Properties {
                        image: "localhost:5000/pipestack/in-internal:0.0.1".to_string(),
                        config: vec![],
                    },
                    traits: vec![
                        Trait {
                            trait_type: "spreadscaler".to_string(),
                            properties: TraitProperties::Spreadscaler { 
                                instances: step.instances.unwrap_or(1) 
                            },
                        },
                        Trait {
                            trait_type: "link".to_string(),
                            properties: TraitProperties::Link(LinkProperties {
                                name: None,
                                target: LinkTarget::Name { name: "messaging-nats".to_string() },
                                namespace: "wasmcloud".to_string(),
                                package: "messaging".to_string(),
                                interfaces: vec!["consumer".to_string()],
                                source: None,
                            }),
                        },
                    Trait {
                        trait_type: "link".to_string(),
                        properties: TraitProperties::Link(LinkProperties {
                            name: None,
                            target: LinkTarget::String(step.name.clone()),
                            namespace: "pipestack".to_string(),
                            package: "out".to_string(),
                            interfaces: vec!["out".to_string()],
                            source: None,
                        }),
                    },
                    ],
                });
                
                // Add the out-log component itself
                components.push(Component {
                    name: step.name.clone(),
                    component_type: "component".to_string(),
                    properties: Properties {
                        image: "localhost:5000/pipestack/out-log:0.0.1".to_string(),
                        config: vec![],
                    },
                    traits: vec![
                        Trait {
                            trait_type: "spreadscaler".to_string(),
                            properties: TraitProperties::Spreadscaler { 
                                instances: step.instances.unwrap_or(1) 
                            },
                        },
                    ],
                });
            },
            _ => {
                // Handle other step types as needed
            }
        }
    }
    
    // Add capabilities (httpserver and messaging-nats)
    // HTTP Server capability
    if pipeline.steps.iter().any(|s| matches!(s.step_type, PipelineStepType::InHttp)) {
        let http_step = pipeline.steps.iter().find(|s| matches!(s.step_type, PipelineStepType::InHttp)).unwrap();
        
        components.push(Component {
            name: "httpserver".to_string(),
            component_type: "capability".to_string(),
            properties: Properties {
                image: "ghcr.io/wasmcloud/http-server:0.27.0".to_string(),
                config: vec![Config {
                    name: "default-http-config".to_string(),
                    properties: {
                        let mut props = BTreeMap::new();
                        props.insert("routing_mode".to_string(), serde_yaml::Value::String("path".to_string()));
                        props.insert("address".to_string(), serde_yaml::Value::String("0.0.0.0:7000".to_string()));
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
                        name: Some(format!("{}-link", http_step.name)),
                        target: LinkTarget::Name { name: http_step.name.clone() },
                        namespace: "wasi".to_string(),
                        package: "http".to_string(),
                        interfaces: vec!["incoming-handler".to_string()],
                        source: Some(LinkSource {
                            config: vec![Config {
                                name: "path".to_string(),
                                properties: {
                                    let mut props = BTreeMap::new();
                                    props.insert("path".to_string(), 
                                        serde_yaml::Value::String(format!("/{}", pipeline.name)));
                                    props
                                },
                            }],
                        }),
                    }),
                },
            ],
        });
    }
    
    // NATS messaging capability
    let mut nats_traits = vec![
        Trait {
            trait_type: "spreadscaler".to_string(),
            properties: TraitProperties::Spreadscaler { instances: 5 },
        }
    ];
    
    // Add link traits for each step that needs messaging in the correct order
    let mut link_counter = 'a' as u8;
    
    // First add the processor step link
    for step in &pipeline.steps {
        if matches!(step.step_type, PipelineStepType::Processor) {
            if let Some(topic) = step_topics.get(&step.name) {
                nats_traits.push(Trait {
                    trait_type: "link".to_string(),
                    properties: TraitProperties::Link(LinkProperties {
                        name: Some((link_counter as char).to_string()),
                        target: LinkTarget::Name { name: format!("in-internal-for-{}", step.name) },
                        namespace: "wasmcloud".to_string(),
                        package: "messaging".to_string(),
                        interfaces: vec!["handler".to_string()],
                        source: Some(LinkSource {
                            config: vec![Config {
                                name: format!("subscription-{}", link_counter - b'a' + 1),
                                properties: {
                                    let mut props = BTreeMap::new();
                                    props.insert("subscriptions".to_string(), 
                                        serde_yaml::Value::String(topic.clone()));
                                    props
                                },
                            }],
                        }),
                    }),
                });
                link_counter += 1;
            }
        }
    }
    
    // Then add the out-log step links
    for step in &pipeline.steps {
        if matches!(step.step_type, PipelineStepType::OutLog) {
            if let Some(topic) = step_topics.get(&step.name) {
                nats_traits.push(Trait {
                    trait_type: "link".to_string(),
                    properties: TraitProperties::Link(LinkProperties {
                        name: Some((link_counter as char).to_string()),
                        target: LinkTarget::Name { name: format!("in-internal-for-{}", step.name) },
                        namespace: "wasmcloud".to_string(),
                        package: "messaging".to_string(),
                        interfaces: vec!["handler".to_string()],
                        source: Some(LinkSource {
                            config: vec![Config {
                                name: format!("subscription-{}", link_counter - b'a' + 1),
                                properties: {
                                    let mut props = BTreeMap::new();
                                    props.insert("subscriptions".to_string(), 
                                        serde_yaml::Value::String(topic.clone()));
                                    props
                                },
                            }],
                        }),
                    }),
                });
                link_counter += 1;
            }
        }
    }
    
    components.push(Component {
        name: "messaging-nats".to_string(),
        component_type: "capability".to_string(),
        properties: Properties {
            image: "ghcr.io/wasmcloud/messaging-nats:0.27.0".to_string(),
            config: vec![],
        },
        traits: nats_traits,
    });
    
    Ok(WadmApplication {
        api_version: "core.oam.dev/v1beta1".to_string(),
        kind: "Application".to_string(),
        metadata: Metadata {
            name: pipeline.name.clone(),
            annotations: {
                let mut annotations = BTreeMap::new();
                annotations.insert("version".to_string(), 
                    pipeline.version.clone().unwrap_or_else(|| "0.0.1".to_string()));
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
    fn test_convert_pipeline() {
        let input_yaml = r#"
name: untitled-pipeline
version: 0.0.1
steps:
  - name: in-http_http_1_1750048123367
    type: in-http
  - name: processor_wasm_2_1750048126167
    type: processor
    source: file:///path/to/data-processor.wasm
    instances: 4
    depends_on:
      - in-http_http_1_1750048123367
  - name: out-log_log_3_1750048128320
    type: out-log
    depends_on:
      - processor_wasm_2_1750048126167
  - name: out-log_log_4_1750048130049
    type: out-log
    depends_on:
      - processor_wasm_2_1750048126167
"#;

        let expected_yaml = r#"apiVersion: core.oam.dev/v1beta1
kind: Application
metadata:
  name: untitled-pipeline
  annotations:
    version: 0.0.1
spec:
  components:
  - name: in-http_http_1_1750048123367
    type: component
    properties:
      image: localhost:5000/pipestack/in-http:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target: out-internal-for-in-http_http_1_1750048123367
        namespace: pipestack
        package: out
        interfaces:
        - out
  - name: out-internal-for-in-http_http_1_1750048123367
    type: component
    properties:
      image: localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-in-http_http_1_1750048123367-config
        properties:
          next-step-topic: untitled-pipeline-step-2-in
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
      config:
      - name: processor_wasm_2_1750048126167-config
        properties:
          next-step-topic: untitled-pipeline-step-3-in
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
        instances: 4
  - name: out-internal-for-processor_wasm_2_1750048126167
    type: component
    properties:
      image: localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-processor_wasm_2_1750048126167-config
        properties:
          next-step-topic: untitled-pipeline-step-3-in
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
  - name: in-internal-for-out-log_log_4_1750048130049
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
        target: out-log_log_4_1750048130049
        namespace: pipestack
        package: out
        interfaces:
        - out
  - name: out-log_log_4_1750048130049
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
          address: 0.0.0.0:7000
          routing_mode: path
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        name: in-http_http_1_1750048123367-link
        target:
          name: in-http_http_1_1750048123367
        namespace: wasi
        package: http
        interfaces:
        - incoming-handler
        source:
          config:
          - name: path
            properties:
              path: /untitled-pipeline
  - name: messaging-nats
    type: capability
    properties:
      image: ghcr.io/wasmcloud/messaging-nats:0.27.0
    traits:
    - type: spreadscaler
      properties:
        instances: 5
    - type: link
      properties:
        name: a
        target:
          name: in-internal-for-processor_wasm_2_1750048126167
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
        source:
          config:
          - name: subscription-1
            properties:
              subscriptions: untitled-pipeline-step-2-in
    - type: link
      properties:
        name: b
        target:
          name: in-internal-for-out-log_log_3_1750048128320
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
        source:
          config:
          - name: subscription-2
            properties:
              subscriptions: untitled-pipeline-step-3-in
    - type: link
      properties:
        name: c
        target:
          name: in-internal-for-out-log_log_4_1750048130049
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
        source:
          config:
          - name: subscription-3
            properties:
              subscriptions: untitled-pipeline-step-3-in
"#;

        // Parse the input YAML into a Pipeline struct
        let pipeline: Pipeline = serde_yaml::from_str(input_yaml).expect("Failed to parse input YAML");
        
        // Convert the pipeline to WadmApplication
        let wadm_app = convert_pipeline(&pipeline).expect("Failed to convert pipeline");
        
        // Convert back to YAML
        let output_yaml = serde_yaml::to_string(&wadm_app).expect("Failed to serialize WadmApplication to YAML");
        
        // Compare with expected output
        assert_eq!(output_yaml.trim(), expected_yaml.trim());
    }
    
    #[test]
    fn test_convert_pipeline_2() {
        let input_yaml = r#"
name: untitled-pipeline
version: 0.0.1
steps:
  - name: in-http_http_1_1750048123367
    type: in-http
  - name: processor_wasm_2_1750048126167
    type: processor
    source: file:///path/to/data-processor.wasm
    instances: 4
    depends_on:
      - in-http_http_1_1750048123367
  - name: out-log_log_3_1750048128320
    type: out-log
    depends_on:
      - processor_wasm_2_1750048126167
"#;

        let expected_yaml = r#"apiVersion: core.oam.dev/v1beta1
kind: Application
metadata:
  name: untitled-pipeline
  annotations:
    version: 0.0.1
spec:
  components:
  - name: in-http_http_1_1750048123367
    type: component
    properties:
      image: localhost:5000/pipestack/in-http:0.0.1
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        target: out-internal-for-in-http_http_1_1750048123367
        namespace: pipestack
        package: out
        interfaces:
        - out
  - name: out-internal-for-in-http_http_1_1750048123367
    type: component
    properties:
      image: localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-in-http_http_1_1750048123367-config
        properties:
          next-step-topic: untitled-pipeline-step-2-in
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
      config:
      - name: processor_wasm_2_1750048126167-config
        properties:
          next-step-topic: untitled-pipeline-step-3-in
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
        instances: 4
  - name: out-internal-for-processor_wasm_2_1750048126167
    type: component
    properties:
      image: localhost:5000/pipestack/out-internal:0.0.1
      config:
      - name: out-internal-for-processor_wasm_2_1750048126167-config
        properties:
          next-step-topic: untitled-pipeline-step-3-in
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
          address: 0.0.0.0:7000
          routing_mode: path
    traits:
    - type: spreadscaler
      properties:
        instances: 1
    - type: link
      properties:
        name: in-http_http_1_1750048123367-link
        target:
          name: in-http_http_1_1750048123367
        namespace: wasi
        package: http
        interfaces:
        - incoming-handler
        source:
          config:
          - name: path
            properties:
              path: /untitled-pipeline
  - name: messaging-nats
    type: capability
    properties:
      image: ghcr.io/wasmcloud/messaging-nats:0.27.0
    traits:
    - type: spreadscaler
      properties:
        instances: 5
    - type: link
      properties:
        name: a
        target:
          name: in-internal-for-processor_wasm_2_1750048126167
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
        source:
          config:
          - name: subscription-1
            properties:
              subscriptions: untitled-pipeline-step-2-in
    - type: link
      properties:
        name: b
        target:
          name: in-internal-for-out-log_log_3_1750048128320
        namespace: wasmcloud
        package: messaging
        interfaces:
        - handler
        source:
          config:
          - name: subscription-2
            properties:
              subscriptions: untitled-pipeline-step-3-in
"#;

        // Parse the input YAML into a Pipeline struct
        let pipeline: Pipeline = serde_yaml::from_str(input_yaml).expect("Failed to parse input YAML");
        
        // Convert the pipeline to WadmApplication
        let wadm_app = convert_pipeline(&pipeline).expect("Failed to convert pipeline");
        
        // Convert back to YAML
        let output_yaml = serde_yaml::to_string(&wadm_app).expect("Failed to serialize WadmApplication to YAML");
        
        // Compare with expected output
        assert_eq!(output_yaml.trim(), expected_yaml.trim());
    }
}

