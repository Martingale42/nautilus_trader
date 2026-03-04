pub fn gateway_http_url(base_url: &str) -> String {
    format!("{base_url}/api")
}

pub fn gateway_ws_url(base_url: &str) -> String {
    if let Some(host) = base_url.strip_prefix("http://") {
        format!("ws://{host}/ws")
    } else if let Some(host) = base_url.strip_prefix("https://") {
        format!("wss://{host}/ws")
    } else {
        format!("{base_url}/ws")
    }
}
