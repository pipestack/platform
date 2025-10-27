pub mod r#in;
pub mod out;
pub mod processor;
pub mod registry;

pub const NODE_IN_HTTP_NAME: &str = "in_http_s.wasm";
pub const NODE_IN_HTTP_VERSION: &str = "0.1.7";
pub const NODE_IN_INTERNAL_NAME: &str = "in_internal_s.wasm";
pub const NODE_IN_INTERNAL_VERSION: &str = "0.1.8";
pub const NODE_OUT_HTTP_WEBHOOK_NAME: &str = "out_http_webhook_s.wasm";
pub const NODE_OUT_HTTP_WEBHOOK_VERSION: &str = "0.1.7";
pub const NODE_OUT_INTERNAL_NAME: &str = "out_internal_s.wasm";
pub const NODE_OUT_INTERNAL_VERSION: &str = "0.1.7";
pub const NODE_OUT_LOG_NAME: &str = "out_log_s.wasm";
pub const NODE_OUT_LOG_VERSION: &str = "0.1.8";
