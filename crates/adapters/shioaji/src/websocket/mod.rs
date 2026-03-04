pub mod client;
pub mod error;
pub mod handler;
pub mod messages;
pub mod parse;

/// Internal command sent from client API to the background handler task.
pub(crate) enum WsCommand {
    Subscribe { code: String, quote_type: String },
    Unsubscribe { code: String, quote_type: String },
    Close,
}
