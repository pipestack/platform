wit_bindgen::generate!({ generate_all });

use wasmcloud_component::{
    http,
    // wasi::logging::logging::{log, Level},
    // wasmcloud::messaging::{consumer, types},
};

struct Component;

http::export!(Component);

impl http::Server for Component {
    fn handle(
        _request: http::IncomingRequest,
    ) -> http::Result<http::Response<impl http::OutgoingBody>> {
        // let out_internal = wasmcloud::bus::lattice::CallTargetInterface::new("pipestack", "out-internal", "out-internal");

        let received = pipestack::out::out::run("Message from in-http");

        // let subject = wasi::config::runtime::get("topic-next-step")
        //     .expect("Unable to fetch value")
        //     .unwrap_or_else(|| "config value not set".to_string());

        // if let Err(err) = consumer::publish(&types::BrokerMessage {
        //     subject: subject.clone(),
        //     reply_to: None,
        //     body: String::from("A body sent from in-http").into_bytes(),
        // }) {
        //     log(
        //         Level::Error,
        //         "in-http",
        //         format!("Failed to publish message: {:?}", err).as_str(),
        //     );
        // }
        // log(
        //     Level::Info,
        //     "in-http",
        //     format!("Successfully posted a message with subject: {subject:?}").as_str(),
        // );

        Ok(http::Response::new(format!(
            "Hello from Wasm Rust! Message received: {received}\n"
        )))
    }
}
