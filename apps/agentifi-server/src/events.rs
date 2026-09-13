//! Events fanned out to every connected desktop over SSE.

use serde_json::Value;

/// One server-sent event: a name plus its JSON payload.
#[derive(Clone, Debug)]
pub struct ServerEvent {
    pub event: String,
    pub data: Value,
}

impl ServerEvent {
    pub fn new(event: &str, data: Value) -> Self {
        Self {
            event: event.to_owned(),
            data,
        }
    }
}
