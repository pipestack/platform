use shared::PipelineNodeType;

use crate::builders::{
    ComponentBuilder,
    nodes::r#in::InHttpWebhookBuilder,
    nodes::out::{OutHttpWebhookBuilder, OutLogBuilder},
    nodes::processor::ProcessorWasmBuilder,
};

pub struct ComponentBuilderRegistry {
    in_http_webhook: InHttpWebhookBuilder,
    processor_wasm: ProcessorWasmBuilder,
    out_log: OutLogBuilder,
    out_http_webhook: OutHttpWebhookBuilder,
}

impl ComponentBuilderRegistry {
    pub fn new() -> Self {
        Self {
            in_http_webhook: InHttpWebhookBuilder,
            processor_wasm: ProcessorWasmBuilder,
            out_log: OutLogBuilder,
            out_http_webhook: OutHttpWebhookBuilder,
        }
    }

    pub fn get_builder(&self, node_type: &PipelineNodeType) -> Option<&dyn ComponentBuilder> {
        match node_type {
            PipelineNodeType::InHttpWebhook => Some(&self.in_http_webhook),
            PipelineNodeType::ProcessorWasm => Some(&self.processor_wasm),
            PipelineNodeType::OutLog => Some(&self.out_log),
            PipelineNodeType::OutHttpWebhook => Some(&self.out_http_webhook),
            _ => None,
        }
    }
}

impl Default for ComponentBuilderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
