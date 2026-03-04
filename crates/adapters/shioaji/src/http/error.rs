use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum ShioajiHttpError {
    #[error("HTTP request failed: {0}")]
    NetworkError(String),
    #[error("JSON deserialization failed: {0}")]
    JsonError(String),
    #[error("Gateway error ({status}): {body}")]
    GatewayError { status: u16, body: String },
    #[error("Gateway not connected")]
    NotConnected,
}

impl From<serde_json::Error> for ShioajiHttpError {
    fn from(e: serde_json::Error) -> Self {
        Self::JsonError(e.to_string())
    }
}

impl From<nautilus_network::http::HttpClientError> for ShioajiHttpError {
    fn from(e: nautilus_network::http::HttpClientError) -> Self {
        Self::NetworkError(e.to_string())
    }
}
