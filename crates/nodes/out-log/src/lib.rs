use crate::exports::pipestack::out::out::Guest;
use wasmcloud_component::info;

wit_bindgen::generate!({ generate_all });

struct Component;

impl Guest for Component {
    fn run(input: String) -> String {
        info!("Final log message: {input}");
        String::from("Done")
    }
}

export!(Component);
