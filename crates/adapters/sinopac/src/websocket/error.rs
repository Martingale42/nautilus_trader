use thiserror::Error;

pub type SinopacWsResult<T> = Result<T, SinopacWsError>;

#[derive(Debug, Clone, Error)]
pub enum SinopacWsError {
    #[error("WebSocket not connected")]
    NotConnected,
    #[error("Send failed: {0}")]
    Send(String),
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("JSON error: {0}")]
    Json(String),
}
