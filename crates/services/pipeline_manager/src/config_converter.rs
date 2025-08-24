use shared::{Pipeline, PipelineNodeType};
use std::collections::{BTreeMap, HashMap};

use crate::builders::{
    ApplicationRef, BuildContext, Component, Config, LinkProperties, LinkSource, LinkTarget,
    Metadata, Properties, Spec, Trait, TraitProperties, WadmApplication,
    nodes::registry::ComponentBuilderRegistry, providers::ProviderBuilderRegistry,
};
use crate::settings::Settings;

pub fn convert_pipeline(
    pipeline: &Pipeline,
    workspace_slug: &String,
    settings: &Settings,
) -> Result<WadmApplication, Box<dyn std::error::Error>> {
    let mut components = Vec::new();
    let step_topics = determine_step_topics(pipeline, workspace_slug);

    // Create build context
    let context = BuildContext::new(pipeline, workspace_slug, settings, &step_topics);

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
                    name: format!("{workspace_slug}-providers"),
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
                        config: Some(vec![Config {
                            name: format!(
                                "{}-{}-httpserver-path-config-v{}",
                                workspace_slug, pipeline.name, pipeline.version
                            ),
                            properties: {
                                let mut props = BTreeMap::new();
                                props.insert(
                                    "path".to_string(),
                                    serde_yaml::Value::String(format!("/{}", pipeline.name)),
                                );
                                props
                            },
                        }]),
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
            && let Some(topic) = step_topics.get(&step.name)
        {
            nats_traits.push(Trait {
                trait_type: "link".to_string(),
                properties: TraitProperties::Link(LinkProperties {
                    name: Some(format!(
                        "messaging-nats-to-{}-in-internal-for-{}-link",
                        workspace_slug, step.name
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
                                        settings.nats.cluster_uris.to_string(),
                                    ),
                                );
                                props
                            },
                        }]),
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

    for step in &pipeline.nodes {
        if matches!(
            step.step_type,
            PipelineNodeType::OutLog | PipelineNodeType::OutHttpWebhook
        ) && let Some(topic) = step_topics.get(&step.name)
        {
            nats_traits.push(Trait {
                trait_type: "link".to_string(),
                properties: TraitProperties::Link(LinkProperties {
                    name: Some(format!(
                        "messaging-nats-to-{}-in-internal-for-{}-link",
                        workspace_slug, step.name
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
                                        settings.nats.cluster_uris.to_string(),
                                    ),
                                );
                                props
                            },
                        }]),
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
            node_depths.insert(step.name.clone(), 1);
        }
    }

    // Calculate depths for dependent nodes
    let mut changed = true;
    while changed {
        changed = false;
        for step in &pipeline.nodes {
            if let Some(depends_on) = &step.depends_on
                && !depends_on.is_empty()
                && !node_depths.contains_key(&step.name)
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
                    node_depths.insert(step.name.clone(), max_depth + 1);
                    changed = true;
                }
            }
        }
    }

    // Generate topics for nodes that have dependencies
    for step in &pipeline.nodes {
        if let Some(depends_on) = &step.depends_on
            && !depends_on.is_empty()
            && let Some(&depth) = node_depths.get(&step.name)
        {
            let topic = format!(
                "pipestack.{}.{}.step-{}-in",
                workspace_slug, pipeline.name, depth
            );
            step_topics.insert(step.name.clone(), topic);
        }
    }
    step_topics
}

pub fn create_providers_wadm(workspace_slug: &str, settings: &Settings) -> WadmApplication {
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
        match provider_builder.build_component(workspace_slug, settings) {
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
      id: default_mine-in-http-webhook_17
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
      id: default_mine-out-internal-for-in-http-webhook_17
      config:
      - name: out-internal-for-in-http-webhook_17-config-v1
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
      id: default_mine-in-internal-for-processor-wasm_18
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
      id: default_mine-processor-wasm_18
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: out-internal-for-processor-wasm_18
    type: component
    properties:
      image: http://localhost:5000/pipestack/out-internal:0.0.1
      id: default_mine-out-internal-for-processor-wasm_18
      config:
      - name: out-internal-for-processor-wasm_18-config-v1
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
      id: default_mine-in-internal-for-out-log_19
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
      id: default_mine-out-log_19
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
          - name: default-mine-httpserver-path-config-v1
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
          - name: subscription-1-config-v1
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
          - name: subscription-2-config-v1
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
      id: default_mine-in-http-webhook_17
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
      id: default_mine-out-internal-for-in-http-webhook_17
      config:
      - name: out-internal-for-in-http-webhook_17-config-v1
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
      id: default_mine-in-internal-for-processor-wasm_18
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
      id: default_mine-processor-wasm_18
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: out-internal-for-processor-wasm_18
    type: component
    properties:
      image: http://localhost:5000/pipestack/out-internal:0.0.1
      id: default_mine-out-internal-for-processor-wasm_18
      config:
      - name: out-internal-for-processor-wasm_18-config-v1
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
      id: default_mine-in-internal-for-out-log_19
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
      id: default_mine-out-log_19
    traits:
    - type: spreadscaler
      properties:
        instances: 1
  - name: in-internal-for-out-log_20
    type: component
    properties:
      image: http://localhost:5000/pipestack/in-internal:0.0.1
      id: default_mine-in-internal-for-out-log_20
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
      id: default_mine-out-log_20
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
          - name: default-mine-httpserver-path-config-v1
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
          - name: subscription-1-config-v1
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
          - name: subscription-2-config-v1
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
          - name: subscription-3-config-v1
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

    #[test]
    fn test_provider_registry() {
        use crate::builders::providers::ProviderBuilderRegistry;

        let settings = Settings {
            cloudflare: crate::settings::Cloudflare {
                account_id: "test_account".to_string(),
                r2_access_key_id: "test_key".to_string(),
                r2_secret_access_key: "test_secret".to_string(),
                r2_bucket: "test_bucket".to_string(),
            },
            nats: crate::settings::Nats {
                cluster_uris: "nats://localhost:4222".to_string(),
            },
            registry: crate::settings::Registry {
                url: "http://localhost:8080".to_string(),
            },
        };

        let registry = ProviderBuilderRegistry::new();
        let workspace_slug = "test-workspace";

        // Test that all providers can be built successfully
        let mut component_names = Vec::new();
        for provider_builder in registry.get_all_providers() {
            match provider_builder.build_component(workspace_slug, &settings) {
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
        let settings = Settings {
            cloudflare: crate::settings::Cloudflare {
                account_id: "test_account".to_string(),
                r2_access_key_id: "test_key".to_string(),
                r2_secret_access_key: "test_secret".to_string(),
                r2_bucket: "test_bucket".to_string(),
            },
            nats: crate::settings::Nats {
                cluster_uris: "nats://localhost:4222".to_string(),
            },
            registry: crate::settings::Registry {
                url: "http://localhost:8080".to_string(),
            },
        };

        let wadm_app = create_providers_wadm("test-workspace", &settings);

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

        let settings = Settings {
            cloudflare: crate::settings::Cloudflare {
                account_id: "test_account".to_string(),
                r2_access_key_id: "test_key".to_string(),
                r2_secret_access_key: "test_secret".to_string(),
                r2_bucket: "test_bucket".to_string(),
            },
            nats: crate::settings::Nats {
                cluster_uris: "nats://localhost:4222".to_string(),
            },
            registry: crate::settings::Registry {
                url: "http://localhost:8080".to_string(),
            },
        };

        let registry = ProviderBuilderRegistry::new();
        let workspace_slug = "test-workspace";

        // Test individual provider builders using the get_builder method
        if let Some(http_server_builder) = registry.get_builder(&ProviderType::HttpServer) {
            let component = http_server_builder
                .build_component(workspace_slug, &settings)
                .unwrap();
            assert_eq!(component.name, "httpserver");
        }

        if let Some(http_client_builder) = registry.get_builder(&ProviderType::HttpClient) {
            let component = http_client_builder
                .build_component(workspace_slug, &settings)
                .unwrap();
            assert_eq!(component.name, "httpclient");
        }

        if let Some(nats_builder) = registry.get_builder(&ProviderType::NatsMessaging) {
            let component = nats_builder
                .build_component(workspace_slug, &settings)
                .unwrap();
            assert_eq!(component.name, "messaging-nats");
        }
    }
}
