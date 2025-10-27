use bindings::exports::pipestack::out::out::Guest;
use wasmcloud_component::info;

mod bindings {
    use super::Component;
    wit_bindgen::generate!({ generate_all });
    export!(Component);
}

struct Component;

const LOG_CONTEXT: &str = "out-internal";

impl Guest for Component {
    fn run(input: String) -> String {
        info!(context: LOG_CONTEXT, "{input}");
        String::from("OK")
    }
}
