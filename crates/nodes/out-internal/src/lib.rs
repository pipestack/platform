use bindings::exports::pipestack::out::out::Guest;

use bindings::wasmcloud::messaging::{consumer, types};
use wasmcloud_component::{error, trace};

mod bindings {
    use super::Component;
    wit_bindgen::generate!({ generate_all });
    export!(Component);
}

struct Component;

const LOG_CONTEXT: &str = "out-internal";

impl Guest for Component {
    fn run(input: String) -> String {
        let subject = bindings::wasi::config::runtime::get("next-step-topic")
            .expect("Unable to fetch value")
            .unwrap_or_else(|| "config value not set".to_string());

        if let Err(err) = consumer::publish(&types::BrokerMessage {
            subject: subject.clone(),
            reply_to: None,
            body: input.into_bytes(),
        }) {
            error!(context: LOG_CONTEXT, "Failed to publish message: {err:?}");
        } else {
            trace!(context: LOG_CONTEXT, "Successfully posted a message to subject: {subject:?}");
        }

        "OK".to_string()
    }
}
