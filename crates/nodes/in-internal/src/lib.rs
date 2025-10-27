use bindings::{exports::wasmcloud::messaging, wasmcloud::messaging::types::BrokerMessage};
use wasmcloud_component::{error, info, trace};

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
            Ok(Ok(res)) => {
                info!(context: LOG_CONTEXT,"Called customer code: {res}");
                res
            }
            Ok(Err(err)) => {
                error!(context: LOG_CONTEXT,
                    "Error calling customer code: {err:?}. Using original message as fallback."
                );
                return Err(format!("{err:?}"));
            }
            Err(_err) => {
                trace!(context: LOG_CONTEXT, "AAA   Custom code not linked, using original message. This is expected for in-internal nodes that are not linking to a processor-* component.");
                String::from_utf8(msg.body.clone()).unwrap()
            }
        };

        info!(context: LOG_CONTEXT,"Calling out");
        let received = bindings::pipestack::out::out::run(response_from_custom_code.as_str());
        info!(context: LOG_CONTEXT,"Called out. Return value: {received}");
        Ok(())
    }
}
