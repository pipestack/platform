use crate::exports::pipestack::out::out::Guest;

use wasi::logging::logging::{log, Level};
use wasmcloud::messaging::{consumer, types};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn run(input: String) -> String {
        let subject = wasi::config::runtime::get("next-step-topic")
            .expect("Unable to fetch value")
            .unwrap_or_else(|| "config value not set".to_string());

        if let Err(err) = consumer::publish(&types::BrokerMessage {
            subject: subject.clone(),
            reply_to: None,
            body: input.into_bytes(),
        }) {
            log(
                Level::Error,
                "in-http",
                format!("Failed to publish message: {err:?}").as_str(),
            );
        }
        log(
            Level::Info,
            "in-http",
            format!("Successfully posted a message with subject: {subject:?}").as_str(),
        );

        "Done".to_string()
    }
}

export!(Component);
