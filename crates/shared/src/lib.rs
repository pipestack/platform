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
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH, optional_fields)]
pub struct Authentication {
    #[serde(rename = "type")]
    pub auth_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<AuthenticationConfig>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH, optional_fields)]
pub struct Validation {
    pub timeout: u16,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH, optional_fields)]
pub struct ProcessorWasmSettings {
    pub source: String,
    pub instances: u32,
}
impl FromConfig for ProcessorWasmSettings {}

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
#[ts(export, export_to = PIPELINE_TS_FILE_PATH)]
pub struct NoSettings;
impl FromConfig for NoSettings {}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[serde(tag = "type", content = "settings")]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH)]
pub enum PipelineNodeSettings {
    // Sources - Cloud Storages
    #[serde(rename = "in-aws-s3")]
    InAwsS3(NoSettings),
    #[serde(rename = "in-google-gcs")]
    InGoogleGcs(NoSettings),
    #[serde(rename = "in-azure-blob")]
    InAzureBlob(NoSettings),

    // Sources - Databases
    #[serde(rename = "in-postgresql")]
    InPostgresql(NoSettings),
    #[serde(rename = "in-mongodb")]
    InMongodb(NoSettings),
    #[serde(rename = "in-mysql")]
    InMysql(NoSettings),
    #[serde(rename = "in-sqlite")]
    InSqlite(NoSettings),

    // Sources - Streaming
    #[serde(rename = "in-kafka")]
    InKafka(NoSettings),
    #[serde(rename = "in-nats")]
    InNats(NoSettings),
    #[serde(rename = "in-rabbitmq")]
    InRabbitmq(NoSettings),
    #[serde(rename = "in-redis")]
    InRedis(NoSettings),

    // Sources - Web / API
    #[serde(rename = "in-http-webhook")]
    InHttpWebhook(InHttpWebhookSettings),
    #[serde(rename = "in-http-poller")]
    InHttpPoller(NoSettings),
    #[serde(rename = "in-graphql-poller")]
    InGraphqlPoller(NoSettings),
    #[serde(rename = "in-rss-reader")]
    InRssReader(NoSettings),

    // Sources - Cloud Services
    #[serde(rename = "in-google-pubsub")]
    InGooglePubsub(NoSettings),
    #[serde(rename = "in-aws-kinesis")]
    InAwsKinesis(NoSettings),
    #[serde(rename = "in-stripe")]
    InStripe(NoSettings),
    #[serde(rename = "in-github-webhook")]
    InGithubWebhook(NoSettings),

    // Processors
    #[serde(rename = "processor-wasm")]
    ProcessorWasm(ProcessorWasmSettings),

    // Sinks - Databases
    #[serde(rename = "out-postgresql")]
    OutPostgresql(NoSettings),
    #[serde(rename = "out-mongodb")]
    OutMongodb(NoSettings),
    #[serde(rename = "out-mysql")]
    OutMysql(NoSettings),
    #[serde(rename = "out-redis")]
    OutRedis(NoSettings),

    // Sinks - Cloud Storages
    #[serde(rename = "out-aws-s3")]
    OutAwsS3(NoSettings),
    #[serde(rename = "out-google-gcs")]
    OutGoogleGcs(NoSettings),
    #[serde(rename = "out-azure-blob")]
    OutAzureBlob(NoSettings),

    // Sinks - Streaming / Queues
    #[serde(rename = "out-kafka")]
    OutKafka(NoSettings),
    #[serde(rename = "out-nats")]
    OutNats(NoSettings),
    #[serde(rename = "out-rabbitmq")]
    OutRabbitmq(NoSettings),
    #[serde(rename = "out-google-pubsub")]
    OutGooglePubsub(NoSettings),

    // Sinks - Web / API
    #[serde(rename = "out-graphql-mutation")]
    OutGraphqlMutation(NoSettings),
    #[serde(rename = "out-slack")]
    OutSlack(NoSettings),
    #[serde(rename = "out-twilio-sms")]
    OutTwilioSms(NoSettings),
    #[serde(rename = "out-http-webhook")]
    OutHttpWebhook(OutHttpWebhookSettings),

    // Sinks - Observability
    #[serde(rename = "out-prometheus")]
    OutPrometheus(NoSettings),
    #[serde(rename = "out-loki")]
    OutLoki(NoSettings),
    #[serde(rename = "out-elasticsearch")]
    OutElasticsearch(NoSettings),
    #[serde(rename = "out-influxdb")]
    OutInfluxdb(NoSettings),

    // Sinks - Cloud Integrations
    #[serde(rename = "out-google-bigquery")]
    OutGoogleBigquery(NoSettings),
    #[serde(rename = "out-snowflake")]
    OutSnowflake(NoSettings),
    #[serde(rename = "out-aws-lambda")]
    OutAwsLambda(NoSettings),
    #[serde(rename = "out-log")]
    OutLog(NoSettings),
}

#[derive(Debug, Deserialize, Serialize, JsonSchema, TS)]
#[ts(export, export_to = PIPELINE_TS_FILE_PATH, optional_fields)]
pub struct PipelineNode {
    pub id: String,
    pub label: String,
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
