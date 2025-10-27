use crate::bindings::{
    exports::pipestack::customer::customer::{Guest, RunError},
    wrpc::rpc,
};

mod bindings {
    use super::Component;
    wit_bindgen::generate!({ generate_all });
    export!(Component);
}

struct Component;

impl Guest for Component {
    fn run(input: String) -> Result<Result<std::string::String, RunError>, rpc::error::Error> {
        Ok(Ok(format!(
            "Received: {input}. Hello there from the nodes/customer stub"
        )))
    }
}
