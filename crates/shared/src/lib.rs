use schemars::JsonSchema;
use serde::{
    Deserialize, Serialize,
    de::{DeserializeOwned, Error},
};
use ts_rs::TS;

const PIPELINE_TS_FILE_PATH: &str = "./pipeline.ts";

#[derive(Debug)]
pub enum ConfigError {
    DeserializationError(serde_json::Error),
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
    fn from_config(config: Option<String>) -> Result<Self, ConfigError> {
        match config {
            Some(config_str) => {
                serde_json::from_str(&config_str).map_err(ConfigError::DeserializationError)
            }
            None => Err(ConfigError::DeserializationError(
                serde_json::Error::custom("No configuration provided"),
            )),
        }
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
    pub path: String,
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
#[ts(export, export_to = PIPELINE_TS_FILE_PATH, optional_fields)]
pub struct HttpHeader {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH, optional_fields)]
pub struct AuthenticationConfig {
    pub location: String,
    pub name: String,
    pub value: String,
    pub prefix: String,
    pub realm: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH, optional_fields)]
pub struct Authentication {
    #[serde(rename = "type")]
    pub auth_type: String,
    pub config: AuthenticationConfig,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH, optional_fields)]
pub struct Validation {
    pub timeout: u16,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH, optional_fields)]
pub struct OutHttpWebhookSettings {
    pub method: String,
    pub url: String,
    #[serde(rename = "contentType", skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Vec<HttpHeader>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authentication: Option<Authentication>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<Validation>,
}
impl FromConfig for OutHttpWebhookSettings {}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[serde(tag = "type", content = "settings")]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH)]
pub enum PipelineNodeSettings {
    #[serde(rename = "in-http-webhook")]
    InHttpWebhook(InHttpWebhookSettings),
    #[serde(rename = "out-http-webhook")]
    OutHttpWebhook(OutHttpWebhookSettings),
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
    OutGraphqlMutation,
    OutSlack,
    OutTwilioSms,
    OutHttpWebhook,
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
