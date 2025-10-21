use shared::{Pipeline, PipelineNodeSettings, PipelineNodeType};
use std::collections::{BTreeMap, HashMap};

use crate::builders::{
    ApplicationRef, BuildContext, Component, Config, LinkProperties, LinkSource, LinkTarget,
    Metadata, Properties, Spec, Trait, TraitProperties, WadmApplication,
    nodes::registry::ComponentBuilderRegistry, providers::ProviderBuilderRegistry,
};
use crate::config::AppConfig;

pub fn convert_pipeline(
    pipeline: &Pipeline,
    workspace_slug: &String,
    app_config: &AppConfig,
) -> Result<WadmApplication, Box<dyn std::error::Error>> {
    let mut components = Vec::new();
    let step_topics = determine_step_topics(pipeline, workspace_slug);

    // Create build context
    let context = BuildContext::new(pipeline, workspace_slug, app_config, &step_topics);

    // Create builder registry
    let registry = ComponentBuilderRegistry::new();

    // Process each step using the appropriate builder
    for step in &pipeline.nodes {
        if let Some(builder) = registry.get_builder(&step.step_type) {
            let step_components = builder.build_components(step, &context)?;
            components.extend(step_components);
        } else {
            // Handle unsupported step types
            eprintln!("Unsupported step type: {:?}", step.step_type);
        }
    }

    // Add capabilities (httpserver, httpclient and messaging-nats)
    // HTTP Server capability
    let http_steps: Vec<_> = pipeline
        .nodes
        .iter()
        .filter(|s| matches!(s.step_type, PipelineNodeType::InHttpWebhook))
        .collect();

    if !http_steps.is_empty() {
        let mut http_traits = Vec::new();

        for http_step in http_steps {
            // Extract path from settings, or use empty string as default
            let path = match &http_step.settings {
                Some(PipelineNodeSettings::InHttpWebhook(settings)) => settings.path.clone(),
                _ => "".to_string(), // Default empty path, will result in just pipeline name
            };

            http_traits.push(Trait {
                trait_type: "link".to_string(),
                properties: TraitProperties::Link(LinkProperties {
                    name: Some(format!(
                        "httpserver-to-{}-{}-link",
                        workspace_slug, http_step.id
                    )),
                    source: Some(LinkSource {
                        config: Some(vec![Config {
                            name: format!(
                                "{}-{}-httpserver-path-{}-config-v{}",
                                workspace_slug, pipeline.name, path, pipeline.version
                            ),
                            properties: {
                                let mut props = BTreeMap::new();
                                let final_path = if path.is_empty() {
                                    format!("/{}", pipeline.name)
                                } else {
                                    format!("/{}/{}", pipeline.name, path)
                                };
                                props.insert(
                                    "path".to_string(),
                                    serde_yaml::Value::String(final_path),
                                );
                                props
                            },
                        }]),
                    }),
                    target: LinkTarget {
                        name: http_step.id.clone(),
                        config: None,
                    },
                    namespace: "wasi".to_string(),
                    package: "http".to_string(),
                    interfaces: vec!["incoming-handler".to_string()],
                }),
            });
        }

        components.push(Component {
            name: "httpserver".to_string(),
            component_type: "capability".to_string(),
            properties: Properties::WithApplication {
                application: ApplicationRef {
                    name: format!("{workspace_slug}-providers"),
                    component: "httpserver".to_string(),
                },
            },
            traits: http_traits,
        });
    }

    // HTTP Client capability
    if pipeline
        .nodes
        .iter()
        .any(|s| matches!(s.step_type, PipelineNodeType::OutHttpWebhook))
    {
        components.push(Component {
            name: "httpclient".to_string(),
            component_type: "capability".to_string(),
            properties: Properties::WithApplication {
                application: ApplicationRef {
                    name: format!("{workspace_slug}-providers"),
                    component: "httpclient".to_string(),
                },
            },
            traits: vec![],
        });
    }

    // NATS messaging capability
    let mut nats_traits = vec![];

    // Add messaging-nats links
    let mut subscription_counter = 1;
    for step in &pipeline.nodes {
        if matches!(step.step_type, PipelineNodeType::ProcessorWasm)
            && let Some(topic) = step_topics.get(&step.id)
        {
            nats_traits.push(Trait {
                trait_type: "link".to_string(),
                properties: TraitProperties::Link(LinkProperties {
                    name: Some(format!(
                        "messaging-nats-to-{}-in-internal-for-{}-link",
                        workspace_slug, step.id
                    )),
                    source: Some(LinkSource {
                        config: Some(vec![Config {
                            name: format!(
                                "subscription-{subscription_counter}-config-v{}",
                                pipeline.version
                            ),
                            properties: {
                                let mut props = BTreeMap::new();
                                props.insert(
                                    "subscriptions".to_string(),
                                    serde_yaml::Value::String(topic.clone()),
                                );
                                props.insert(
                                    "cluster_uris".to_string(),
                                    serde_yaml::Value::String(
                                        app_config.nats.cluster_uris.to_string(),
                                    ),
                                );
                                props
                            },
                        }]),
                    }),
                    target: LinkTarget {
                        name: format!("in-internal-for-{}", step.id),
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

    for step in &pipeline.nodes {
        if matches!(
            step.step_type,
            PipelineNodeType::OutLog | PipelineNodeType::OutHttpWebhook
        ) && let Some(topic) = step_topics.get(&step.id)
        {
            nats_traits.push(Trait {
                trait_type: "link".to_string(),
                properties: TraitProperties::Link(LinkProperties {
                    name: Some(format!(
                        "messaging-nats-to-{}-in-internal-for-{}-link",
                        workspace_slug, step.id
                    )),
                    source: Some(LinkSource {
                        config: Some(vec![Config {
                            name: format!(
                                "subscription-{subscription_counter}-config-v{}",
                                pipeline.version
                            ),
                            properties: {
                                let mut props = BTreeMap::new();
                                props.insert(
                                    "subscriptions".to_string(),
                                    serde_yaml::Value::String(topic.clone()),
                                );
                                props.insert(
                                    "cluster_uris".to_string(),
                                    serde_yaml::Value::String(
                                        app_config.nats.cluster_uris.to_string(),
                                    ),
                                );
                                props
                            },
                        }]),
                    }),
                    target: LinkTarget {
                        name: format!("in-internal-for-{}", step.id),
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

    components.push(Component {
        name: "messaging-nats".to_string(),
        component_type: "capability".to_string(),
        properties: Properties::WithApplication {
            application: ApplicationRef {
                name: format!("{workspace_slug}-providers"),
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

fn determine_step_topics(pipeline: &Pipeline, workspace_slug: &String) -> HashMap<String, String> {
    let mut step_topics = HashMap::new();

    // Generate topic names for inter-step communication based on dependency depth
    // Build a map of node names to their dependency depth
    let mut node_depths = HashMap::new();

    // Find root nodes (no dependencies)
    for step in &pipeline.nodes {
        if step.depends_on.is_none() || step.depends_on.as_ref().unwrap().is_empty() {
            node_depths.insert(step.id.clone(), 1);
        }
    }

    // Calculate depths for dependent nodes
    let mut changed = true;
    while changed {
        changed = false;
        for step in &pipeline.nodes {
            if let Some(depends_on) = &step.depends_on
                && !depends_on.is_empty()
                && !node_depths.contains_key(&step.id)
            {
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
                    node_depths.insert(step.id.clone(), max_depth + 1);
                    changed = true;
                }
            }
        }
    }

    // Generate topics for nodes that have dependencies
    for step in &pipeline.nodes {
        if let Some(depends_on) = &step.depends_on
            && !depends_on.is_empty()
            && let Some(&depth) = node_depths.get(&step.id)
        {
            let topic = format!(
                "pipestack.{}.{}.step-{}-in",
                workspace_slug, pipeline.name, depth
            );
            step_topics.insert(step.id.clone(), topic);
        }
    }
    step_topics
}

pub fn create_providers_wadm(workspace_slug: &str, app_config: &AppConfig) -> WadmApplication {
    let mut annotations = BTreeMap::new();
    annotations.insert(
        "experimental.wasmcloud.dev/shared".to_string(),
        "true".to_string(),
    );
    annotations.insert(
        "description".to_string(),
        format!("Shared providers for the {workspace_slug} workspace"),
    );
    annotations.insert("version".to_string(), "0.8.0".to_string());

    let mut components = Vec::new();

    // Create provider registry
    let registry = ProviderBuilderRegistry::new();

    // Build all provider components using the registry
    for provider_builder in registry.get_all_providers() {
        match provider_builder.build_component(workspace_slug, app_config) {
            Ok(component) => components.push(component),
            Err(e) => {
                eprintln!("Failed to build provider component: {}", e);
            }
        }
    }

    WadmApplication {
        api_version: "core.oam.dev/v1beta1".to_string(),
        kind: "Application".to_string(),
        metadata: Metadata {
            name: format!("{workspace_slug}-providers"),
            annotations,
        },
        spec: Spec { components },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builders::nodes::{
        NODE_VERSION_IN_HTTP, NODE_VERSION_IN_INTERNAL, NODE_VERSION_OUT_INTERNAL,
        NODE_VERSION_OUT_LOG,
    };

    #[test]
    fn test_convert_pipeline_in_processor_out() {
        let input_yaml = r#"
name: mine
version: 1
nodes:
  - id: in-http-webhook_17
    label: in-http-webhook_17
    type: in-http-webhook
    position:
      x: 300
      'y': 180
    settings:
      type: in-http-webhook
      settings:
        method: GET
        path: 'in-http-webhook_17'
  - id: processor-wasm_18
    label: processor-wasm_18
    type: processor-wasm
    position:
      x: 548
      'y': 69
    source: localhost:5000/nodes/data-processor:0.0.1
    instances: 10000
    depends_on:
      - in-http-webhook_17
  - id: out-log_19
    label: out-log_19
    type: out-log
    position:
      x: 660
      'y': 180
    depends_on:
      - processor-wasm_18
"#;

        let expected_yaml = format!(
            r#"apiVersion: core.oam.dev/v1beta1
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
      image: http://localhost:5000/nodes/in_http:{NODE_VERSION_IN_HTTP}
      id: default_mine-in-http-webhook_17
      config:
        - name: in-http-webhook_17-config-v1
          properties:
            json: '{{"method":"GET","path":"in-http-webhook_17"}}'
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
      image: http://localhost:5000/nodes/out_internal:{NODE_VERSION_OUT_INTERNAL}
      id: default_mine-out-internal-for-in-http-webhook_17
      config:
      - name: out-internal-for-in-http-webhook_17-config-v1
        properties:
          next-step-topic: pipestack.default.mine.step-2-in
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
      image: http://localhost:5000/nodes/in_internal:{NODE_VERSION_IN_INTERNAL}
      id: default_mine-in-internal-for-processor-wasm_18
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
      id: default_mine-processor-wasm_18
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
  - name: out-internal-for-processor-wasm_18
    type: component
    properties:
      image: http://localhost:5000/nodes/out_internal:{NODE_VERSION_OUT_INTERNAL}
      id: default_mine-out-internal-for-processor-wasm_18
      config:
      - name: out-internal-for-processor-wasm_18-config-v1
        properties:
          next-step-topic: pipestack.default.mine.step-3-in
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
      image: http://localhost:5000/nodes/in_internal:{NODE_VERSION_IN_INTERNAL}
      id: default_mine-in-internal-for-out-log_19
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
      image: http://localhost:5000/nodes/out_log:{NODE_VERSION_OUT_LOG}
      id: default_mine-out-log_19
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
          - name: default-mine-httpserver-path-in-http-webhook_17-config-v1
            properties:
              path: /mine/in-http-webhook_17
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
          - name: subscription-1-config-v1
            properties:
              subscriptions: pipestack.default.mine.step-2-in
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
          - name: subscription-2-config-v1
            properties:
              subscriptions: pipestack.default.mine.step-3-in
              cluster_uris: localhost:4222
        target:
          name: in-internal-for-out-log_19
        name: messaging-nats-to-default-in-internal-for-out-log_19-link
"#
        );

        let app_config = AppConfig::new().expect("Could not read app config");

        // Parse input
        let pipeline: Pipeline =
            serde_yaml::from_str(input_yaml).expect("Failed to parse input YAML");

        // Convert to WADM
        let actual_wadm = convert_pipeline(&pipeline, &"default".to_string(), &app_config)
            .expect("Failed to convert pipeline");

        // Parse expected output to same struct type
        let expected_wadm: WadmApplication =
            serde_yaml::from_str(expected_yaml.as_str()).expect("Failed to parse expected YAML");

        // STRUCTURED COMPARISON - much more reliable!
        assert_eq!(actual_wadm, expected_wadm);
    }

    #[test]
    fn test_convert_pipeline_in_processor_out_out() {
        let input_yaml = r#"
name: mine
version: 1
nodes:
  - id: in-http-webhook_17
    label: in-http-webhook_17
    type: in-http-webhook
    position:
      x: 300
      'y': 180
    settings:
      type: in-http-webhook
      settings:
        method: GET
        path: 'in-http-webhook_17'
  - id: processor-wasm_18
    label: processor-wasm_18
    type: processor-wasm
    position:
      x: 548
      'y': 69
    source: localhost:5000/nodes/data-processor:0.0.1
    instances: 10000
    depends_on:
      - in-http-webhook_17
  - id: out-log_19
    label: out-log_19
    type: out-log
    position:
      x: 660
      'y': 180
    depends_on:
      - processor-wasm_18
  - id: out-log_20
    label: out-log_20
    type: out-log
    position:
      x: 960
      'y': 180
    depends_on:
      - processor-wasm_18
"#;

        let expected_yaml = format!(
            r#"apiVersion: core.oam.dev/v1beta1
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
      image: http://localhost:5000/nodes/in_http:{NODE_VERSION_IN_HTTP}
      id: default_mine-in-http-webhook_17
      config:
        - name: in-http-webhook_17-config-v1
          properties:
            json: '{{"method":"GET","path":"in-http-webhook_17"}}'
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
      image: http://localhost:5000/nodes/out_internal:{NODE_VERSION_OUT_INTERNAL}
      id: default_mine-out-internal-for-in-http-webhook_17
      config:
      - name: out-internal-for-in-http-webhook_17-config-v1
        properties:
          next-step-topic: pipestack.default.mine.step-2-in
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
      image: http://localhost:5000/nodes/in_internal:{NODE_VERSION_IN_INTERNAL}
      id: default_mine-in-internal-for-processor-wasm_18
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
      id: default_mine-processor-wasm_18
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
  - name: out-internal-for-processor-wasm_18
    type: component
    properties:
      image: http://localhost:5000/nodes/out_internal:{NODE_VERSION_OUT_INTERNAL}
      id: default_mine-out-internal-for-processor-wasm_18
      config:
      - name: out-internal-for-processor-wasm_18-config-v1
        properties:
          next-step-topic: pipestack.default.mine.step-3-in
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
      image: http://localhost:5000/nodes/in_internal:{NODE_VERSION_IN_INTERNAL}
      id: default_mine-in-internal-for-out-log_19
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
      image: http://localhost:5000/nodes/out_log:{NODE_VERSION_OUT_LOG}
      id: default_mine-out-log_19
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
  - name: in-internal-for-out-log_20
    type: component
    properties:
      image: http://localhost:5000/nodes/in_internal:{NODE_VERSION_IN_INTERNAL}
      id: default_mine-in-internal-for-out-log_20
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
      image: http://localhost:5000/nodes/out_log:{NODE_VERSION_OUT_LOG}
      id: default_mine-out-log_20
    traits:
    - type: spreadscaler
      properties:
        instances: 10000
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
          - name: default-mine-httpserver-path-in-http-webhook_17-config-v1
            properties:
              path: /mine/in-http-webhook_17
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
          - name: subscription-1-config-v1
            properties:
              subscriptions: pipestack.default.mine.step-2-in
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
          - name: subscription-2-config-v1
            properties:
              subscriptions: pipestack.default.mine.step-3-in
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
          - name: subscription-3-config-v1
            properties:
              subscriptions: pipestack.default.mine.step-3-in
              cluster_uris: localhost:4222
        target:
          name: in-internal-for-out-log_20
        name: messaging-nats-to-default-in-internal-for-out-log_20-link
"#
        );

        let app_config = AppConfig::new().expect("Could not read app config");

        // Parse input
        let pipeline: Pipeline =
            serde_yaml::from_str(input_yaml).expect("Failed to parse input YAML");

        // Convert to WADM
        let actual_wadm = convert_pipeline(&pipeline, &"default".to_string(), &app_config)
            .expect("Failed to convert pipeline");

        // Parse expected output to same struct type
        let expected_wadm: WadmApplication =
            serde_yaml::from_str(expected_yaml.as_str()).expect("Failed to parse expected YAML");

        // STRUCTURED COMPARISON - much more reliable!
        assert_eq!(actual_wadm, expected_wadm);
    }

    #[test]
    fn test_provider_registry() {
        use crate::builders::providers::ProviderBuilderRegistry;

        let app_config = AppConfig {
            database: crate::config::DatabaseConfig {
                url: "test-db".to_string(),
            },
            cloudflare: crate::config::Cloudflare {
                account_id: "test_account".to_string(),
                r2_access_key_id: "test_key".to_string(),
                r2_secret_access_key: "test_secret".to_string(),
                r2_bucket: "test_bucket".to_string(),
            },
            nats: crate::config::Nats {
                cluster_uris: "nats://localhost:4222".to_string(),
                jwt: Some("test-jwt".to_string()),
                nkey: Some("test-nkey".to_string()),
            },
            registry: crate::config::Registry {
                internal_url: "http://localhost:8080".to_string(),
                url: "http://localhost:8080".to_string(),
            },
        };

        let registry = ProviderBuilderRegistry::new();
        let workspace_slug = "test-workspace";

        // Test that all providers can be built successfully
        let mut component_names = Vec::new();
        for provider_builder in registry.get_all_providers() {
            match provider_builder.build_component(workspace_slug, &app_config) {
                Ok(component) => {
                    component_names.push(component.name.clone());
                    // Verify component structure
                    assert_eq!(component.component_type, "capability");
                    assert!(!component.traits.is_empty());
                }
                Err(e) => panic!("Failed to build provider component: {}", e),
            }
        }

        // Verify we have all expected providers
        component_names.sort();
        let mut expected_names = vec![
            "httpserver".to_string(),
            "httpclient".to_string(),
            "messaging-nats".to_string(),
        ];
        expected_names.sort();
        assert_eq!(component_names, expected_names);
    }

    #[test]
    fn test_create_providers_wadm_uses_registry() {
        let app_config = AppConfig {
            database: crate::config::DatabaseConfig {
                url: "test-db".to_string(),
            },
            cloudflare: crate::config::Cloudflare {
                account_id: "test_account".to_string(),
                r2_access_key_id: "test_key".to_string(),
                r2_secret_access_key: "test_secret".to_string(),
                r2_bucket: "test_bucket".to_string(),
            },
            nats: crate::config::Nats {
                cluster_uris: "nats://localhost:4222".to_string(),
                jwt: Some("test-jwt".to_string()),
                nkey: Some("test-nkey".to_string()),
            },
            registry: crate::config::Registry {
                internal_url: "http://localhost:8080".to_string(),
                url: "http://localhost:8080".to_string(),
            },
        };

        let wadm_app = create_providers_wadm("test-workspace", &app_config);

        // Verify the application structure
        assert_eq!(wadm_app.api_version, "core.oam.dev/v1beta1");
        assert_eq!(wadm_app.kind, "Application");
        assert_eq!(wadm_app.metadata.name, "test-workspace-providers");

        // Verify we have exactly 3 components (the standard providers)
        assert_eq!(wadm_app.spec.components.len(), 3);

        // Verify component names
        let mut component_names: Vec<String> = wadm_app
            .spec
            .components
            .iter()
            .map(|c| c.name.clone())
            .collect();
        component_names.sort();

        let mut expected_names = vec![
            "httpserver".to_string(),
            "httpclient".to_string(),
            "messaging-nats".to_string(),
        ];
        expected_names.sort();

        assert_eq!(component_names, expected_names);
    }

    #[test]
    fn test_individual_provider_builders() {
        use crate::builders::{ProviderType, providers::ProviderBuilderRegistry};

        let app_config = AppConfig {
            database: crate::config::DatabaseConfig {
                url: "test-db".to_string(),
            },
            cloudflare: crate::config::Cloudflare {
                account_id: "test_account".to_string(),
                r2_access_key_id: "test_key".to_string(),
                r2_secret_access_key: "test_secret".to_string(),
                r2_bucket: "test_bucket".to_string(),
            },
            nats: crate::config::Nats {
                cluster_uris: "nats://localhost:4222".to_string(),
                jwt: Some("test-jwt".to_string()),
                nkey: Some("test-nkey".to_string()),
            },
            registry: crate::config::Registry {
                internal_url: "http://localhost:8080".to_string(),
                url: "http://localhost:8080".to_string(),
            },
        };

        let registry = ProviderBuilderRegistry::new();
        let workspace_slug = "test-workspace";

        // Test individual provider builders using the get_builder method
        if let Some(http_server_builder) = registry.get_builder(&ProviderType::HttpServer) {
            let component = http_server_builder
                .build_component(workspace_slug, &app_config)
                .unwrap();
            assert_eq!(component.name, "httpserver");
        }

        if let Some(http_client_builder) = registry.get_builder(&ProviderType::HttpClient) {
            let component = http_client_builder
                .build_component(workspace_slug, &app_config)
                .unwrap();
            assert_eq!(component.name, "httpclient");
        }

        if let Some(nats_builder) = registry.get_builder(&ProviderType::NatsMessaging) {
            let component = nats_builder
                .build_component(workspace_slug, &app_config)
                .unwrap();
            assert_eq!(component.name, "messaging-nats");
        }
    }

    #[test]
    fn test_multiple_http_webhook_nodes() {
        use shared::{
            InHttpWebhookSettings, Pipeline, PipelineNode, PipelineNodeSettings, PipelineNodeType,
            XYPosition,
        };

        let app_config = AppConfig::new().expect("Could not read app config");

        // Create pipeline manually with multiple InHttpWebhook nodes
        let pipeline = Pipeline {
            name: "multi-http".to_string(),
            version: "1".to_string(),
            nodes: vec![
                PipelineNode {
                    id: "webhook-1".to_string(),
                    label: "A webhook 1".to_string(),
                    step_type: PipelineNodeType::InHttpWebhook,
                    position: XYPosition { x: 100.0, y: 100.0 },
                    settings: Some(PipelineNodeSettings::InHttpWebhook(InHttpWebhookSettings {
                        method: "POST".to_string(),
                        path: "api/webhook1".to_string(),
                        content_type: None,
                        request_body_json_schema: None,
                    })),
                    instances: None,
                    depends_on: None,
                },
                PipelineNode {
                    id: "webhook-2".to_string(),
                    label: "A webhook 2".to_string(),
                    step_type: PipelineNodeType::InHttpWebhook,
                    position: XYPosition { x: 200.0, y: 100.0 },
                    settings: Some(PipelineNodeSettings::InHttpWebhook(InHttpWebhookSettings {
                        method: "GET".to_string(),
                        path: "api/webhook2".to_string(),
                        content_type: None,
                        request_body_json_schema: None,
                    })),
                    instances: None,
                    depends_on: None,
                },
                PipelineNode {
                    id: "processor".to_string(),
                    label: "A processor".to_string(),
                    step_type: PipelineNodeType::ProcessorWasm,
                    position: XYPosition { x: 300.0, y: 100.0 },
                    settings: None,
                    instances: Some(1000),
                    depends_on: Some(vec!["webhook-1".to_string(), "webhook-2".to_string()]),
                },
            ],
        };

        // Convert to WADM
        let actual_wadm = convert_pipeline(&pipeline, &"test".to_string(), &app_config)
            .expect("Failed to convert pipeline");

        // Find the httpserver component
        let httpserver_component = actual_wadm
            .spec
            .components
            .iter()
            .find(|c| c.name == "httpserver")
            .expect("Should have httpserver component");

        // Verify that the httpserver has two link traits (one for each webhook)
        assert_eq!(
            httpserver_component.traits.len(),
            2,
            "Should have 2 link traits for 2 webhooks"
        );

        // Verify the first webhook link
        let link1 = &httpserver_component.traits[0];
        assert_eq!(link1.trait_type, "link");
        if let TraitProperties::Link(link_props) = &link1.properties {
            assert_eq!(link_props.target.name, "webhook-1");
            assert_eq!(
                link_props.name,
                Some("httpserver-to-test-webhook-1-link".to_string())
            );

            // Check the path configuration
            if let Some(source) = &link_props.source {
                if let Some(config) = &source.config {
                    assert_eq!(config.len(), 1);
                    let path_value = config[0].properties.get("path").unwrap();
                    assert_eq!(
                        path_value,
                        &serde_yaml::Value::String("/multi-http/api/webhook1".to_string())
                    );
                }
            }
        }

        // Verify the second webhook link
        let link2 = &httpserver_component.traits[1];
        assert_eq!(link2.trait_type, "link");
        if let TraitProperties::Link(link_props) = &link2.properties {
            assert_eq!(link_props.target.name, "webhook-2");
            assert_eq!(
                link_props.name,
                Some("httpserver-to-test-webhook-2-link".to_string())
            );

            // Check the path configuration
            if let Some(source) = &link_props.source {
                if let Some(config) = &source.config {
                    assert_eq!(config.len(), 1);
                    let path_value = config[0].properties.get("path").unwrap();
                    assert_eq!(
                        path_value,
                        &serde_yaml::Value::String("/multi-http/api/webhook2".to_string())
                    );
                }
            }
        }
    }
}
