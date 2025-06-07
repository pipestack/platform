use crate::{exports::pipestack::customer::customer::Guest, wrpc::rpc};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn run(input: String) -> Result<String, rpc::error::Error> {
        Ok(format!(
            "Received: {input}. Hello there from the nodes/customer stub"
        ))
        // TODO: Send the input to the next atep in the pipeline based on configuration
    }
}

export!(Component);
