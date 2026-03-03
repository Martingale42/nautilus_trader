pub fn gateway_http_url(base_url: &str) -> String {
    format!("{base_url}/api")
}

pub fn gateway_ws_url(base_url: &str) -> String {
    if base_url.starts_with("http://") {
        format!("ws://{}/ws", &base_url["http://".len()..])
    } else if base_url.starts_with("https://") {
        format!("wss://{}/ws", &base_url["https://".len()..])
    } else {
        format!("{base_url}/ws")
    }
}
