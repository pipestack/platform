use bindings::{exports::wasmcloud::messaging, wasmcloud::messaging::types::BrokerMessage};
use wasmcloud_component::info;

mod bindings {
    use super::WitComponent;
    wit_bindgen::generate!({ generate_all });
    export!(WitComponent);
}

struct WitComponent;

const LOG_CONTEXT: &str = "in-http";

impl messaging::handler::Guest for WitComponent {
    fn handle_message(msg: BrokerMessage) -> Result<(), String> {
        info!(context: LOG_CONTEXT,
            "Message received in in-internal: {:?}",
            String::from_utf8(msg.body.clone())
        );

        let response_from_custom_code = match bindings::pipestack::customer::customer::run(
            String::from_utf8(msg.body.clone()).unwrap().as_str(),
        ) {
            Ok(res) => {
                info!("Called customer code: {res}");
                res
            }
            // Err(err) => error!("Failed to execute the customer code: {err:?}"),
            Err(_) => String::from_utf8(msg.body.clone()).unwrap(),
        };

        info!(context: LOG_CONTEXT,"Calling out");
        let received = bindings::pipestack::out::out::run(response_from_custom_code.as_str());
        info!(context: LOG_CONTEXT,"Called out. Return value: {received}");
        Ok(())
    }
}
