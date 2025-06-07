use crate::{exports::pipestack::customer::customer::Guest, wrpc::rpc};

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn run(input: String) -> Result<String, rpc::error::Error> {
        Ok(format!(
            "Input: {input}. Howdy from the custom component code!"
        ))
    }
}

export!(Component);
