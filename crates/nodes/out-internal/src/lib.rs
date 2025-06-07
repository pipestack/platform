use crate::exports::pipestack::out::out::Guest;

use wasi::logging::logging::{log, Level};
use wasmcloud::messaging::{consumer, types};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn run(input: String) -> String {
        // let next_step = wasmcloud::bus::lattice::CallTargetInterface::new(
        //     "pipestack",
        //     "in-internal",
        //     "in-internal",
        // );

        // let next_step_link_name = wasi::config::runtime::get("next-step-link-name")
        //     .expect("Unable to fetch value")
        //     .unwrap_or_else(|| "config value not set".to_string());

        // wasmcloud::bus::lattice::set_link_name(next_step_link_name.as_str(), vec![next_step]);

        let subject = wasi::config::runtime::get("next-step-topic")
            .expect("Unable to fetch value")
            .unwrap_or_else(|| "config value not set".to_string());

        if let Err(err) = consumer::publish(&types::BrokerMessage {
            subject: subject.clone(),
            reply_to: None,
            body: String::from("A body sent from in-http").into_bytes(),
        }) {
            log(
                Level::Error,
                "in-http",
                format!("Failed to publish message: {:?}", err).as_str(),
            );
        }
        log(
            Level::Info,
            "in-http",
            format!("Successfully posted a message with subject: {subject:?}").as_str(),
        );

        format!("Received: {input}. Hello there")
        // TODO: Send the input to the next step in the pipeline based on configuration
    }
}

export!(Component);
