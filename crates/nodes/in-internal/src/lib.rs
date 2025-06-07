mod bindings {
    use crate::WitComponent;
    wit_bindgen::generate!({ generate_all });
    export!(WitComponent);
}

struct WitComponent;

use bindings::{exports::wasmcloud::messaging, wasmcloud::messaging::types::BrokerMessage};
use wasmcloud_component::info;

impl messaging::handler::Guest for WitComponent {
    fn handle_message(msg: BrokerMessage) -> Result<(), String> {
        info!(
            "Message received in in-internal: {:?}",
            String::from_utf8(msg.body)
        );

        match bindings::pipestack::customer::customer::run("A message from in-internal") {
            Ok(res) => info!("Called customer code: {res}"),
            // Err(err) => error!("Failed to execute the customer code: {err:?}"),
            Err(_) => (),
        }

        info!("Calling out");
        let received = bindings::pipestack::out::out::run("Message from in-internal");
        info!("Called out. Return value: {received}");
        Ok(())
    }
}
