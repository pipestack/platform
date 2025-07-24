use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use ts_rs::TS;

const PIPELINE_TS_FILE_PATH: &str = "./pipeline.ts";

#[derive(Debug)]
pub enum ConfigError {
    DeserializationError(serde_yaml::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::DeserializationError(err) => {
                write!(f, "Configuration deserialization error: {err}")
            }
        }
    }
}

pub trait FromConfig: DeserializeOwned {
    fn from_config(config: Vec<(String, String)>) -> Result<Self, ConfigError> {
        let mut yaml_map = serde_yaml::Mapping::new();

        for (key, value) in config {
            let yaml_key = serde_yaml::Value::String(key);

            let yaml_value = match serde_yaml::from_str::<serde_yaml::Value>(&value) {
                Ok(parsed) => parsed,
                Err(_) => serde_yaml::Value::String(value),
            };

            yaml_map.insert(yaml_key, yaml_value);
        }

        let yaml_mapping = serde_yaml::Value::Mapping(yaml_map);
        serde_yaml::from_value(yaml_mapping).map_err(ConfigError::DeserializationError)
    }
}

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
pub struct InHttpWebhookSettings {
    pub method: String,
    #[serde(rename = "contentType", skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(
        rename = "requestBodyJsonSchema",
        skip_serializing_if = "Option::is_none"
    )]
    pub request_body_json_schema: Option<serde_json::Value>,
}
impl FromConfig for InHttpWebhookSettings {}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[serde(tag = "type", content = "settings")]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH)]
pub enum PipelineNodeSettings {
    #[serde(rename = "in-http-webhook")]
    InHttpWebhook(InHttpWebhookSettings),
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
    pub settings: Option<PipelineNodeSettings>,
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
