use thiserror::Error;

pub type ShioajiWsResult<T> = Result<T, ShioajiWsError>;

#[derive(Debug, Clone, Error)]
pub enum ShioajiWsError {
    #[error("WebSocket not connected")]
    NotConnected,
    #[error("Send failed: {0}")]
    Send(String),
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("JSON error: {0}")]
    Json(String),
}
