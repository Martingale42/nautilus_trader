use std::path::PathBuf;

pub fn get_test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data")
}

pub fn load_test_json(filename: &str) -> String {
    std::fs::read_to_string(get_test_data_dir().join(filename))
        .unwrap_or_else(|_| panic!("Failed to load test fixture: {filename}"))
}

pub fn load_test_json_as<T: serde::de::DeserializeOwned>(filename: &str) -> T {
    let json = load_test_json(filename);
    serde_json::from_str(&json)
        .unwrap_or_else(|e| panic!("Failed to parse fixture {filename}: {e}"))
}
