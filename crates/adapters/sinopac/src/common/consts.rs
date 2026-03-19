use std::sync::LazyLock;

use nautilus_model::identifiers::Venue;

pub const SINOPAC: &str = "SINOPAC";
pub static SINOPAC_VENUE: LazyLock<Venue> = LazyLock::new(|| Venue::new(SINOPAC));

pub const SINOPAC_GATEWAY_HTTP_URL: &str = "http://localhost:8000";
pub const SINOPAC_GATEWAY_WS_URL: &str = "ws://localhost:8000/ws";
