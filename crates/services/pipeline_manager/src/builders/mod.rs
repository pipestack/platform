use serde::{Deserialize, Serialize};
use shared::{Pipeline, PipelineNode};
use std::collections::{BTreeMap, HashMap};

use crate::config::AppConfig;

pub mod nodes;
pub mod providers;

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
        id: Option<String>,
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
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<Vec<Config>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<Vec<Config>>,
}

/// Context passed to component builders
pub struct BuildContext<'a> {
    pub pipeline: &'a Pipeline,
    pub workspace_slug: &'a str,
    pub app_config: &'a AppConfig,
    pub step_topics: &'a HashMap<String, String>,
}

impl<'a> BuildContext<'a> {
    pub fn new(
        pipeline: &'a Pipeline,
        workspace_slug: &'a str,
        app_config: &'a AppConfig,
        step_topics: &'a HashMap<String, String>,
    ) -> Self {
        Self {
            pipeline,
            workspace_slug,
            app_config,
            step_topics,
        }
    }

    pub fn find_next_step_topic(&self, current_step: &str) -> Option<String> {
        self.pipeline
            .nodes
            .iter()
            .find(|s| {
                s.depends_on
                    .as_ref()
                    .is_some_and(|deps| deps.contains(&current_step.to_string()))
            })
            .and_then(|s| self.step_topics.get(&s.name))
            .cloned()
    }
}

/// Trait for building pipeline components
pub trait ComponentBuilder {
    /// Build components for a given pipeline node
    fn build_components(
        &self,
        step: &PipelineNode,
        context: &BuildContext,
    ) -> Result<Vec<Component>, Box<dyn std::error::Error>>;
}

/// Enum for provider types
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderType {
    HttpServer,
    HttpClient,
    NatsMessaging,
}

/// Trait for building provider components
pub trait ProviderBuilder {
    /// Build a provider component for a given workspace
    fn build_component(
        &self,
        workspace_slug: &str,
        app_config: &AppConfig,
    ) -> Result<Component, Box<dyn std::error::Error>>;
}

/// Helper function to convert settings to config properties
fn settings_to_config_properties<T: serde::Serialize>(
    settings: &T,
) -> BTreeMap<String, serde_yaml::Value> {
    let settings_value = serde_json::to_value(settings).expect("Failed to serialize settings");

    let mut props = BTreeMap::new();
    if let serde_json::Value::Object(map) = settings_value {
        for (key, value) in map {
            let yaml_value = serde_yaml::to_value(value).expect("Failed to convert to YAML value");
            props.insert(key, yaml_value);
        }
    }

    // Convert props to JSON string
    let json_string = match serde_json::to_string(&props) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to serialize properties to JSON: {e}");
            return BTreeMap::new();
        }
    };

    // Parse back to serde_yaml::Value to return as JSON string value
    let mut result = BTreeMap::new();
    result.insert("json".to_string(), serde_yaml::Value::String(json_string));
    result
}
