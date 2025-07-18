wit_bindgen::generate!({ generate_all });

use wasmcloud_component::http;

struct Component;

http::export!(Component);

impl http::Server for Component {
    fn handle(
        request: http::IncomingRequest,
    ) -> http::Result<http::Response<impl http::OutgoingBody>> {
        let message = request
            .uri()
            .query()
            .and_then(|query| {
                query.split('&').find_map(|param| {
                    let mut parts = param.split('=');
                    match (parts.next(), parts.next()) {
                        (Some("message"), Some(value)) => Some(value),
                        _ => None,
                    }
                })
            })
            .unwrap_or("default message");
        let received = pipestack::out::out::run(message);
        Ok(http::Response::new(format!("{received}\n")))
    }
}
