// -------------------------------------------------------------------------------------------------
//  Copyright (C) 2015-2025 Nautech Systems Pty Ltd. All rights reserved.
//  https://nautechsystems.io
//
//  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
//  You may not use this file except in compliance with the License.
//  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
//
//  Unless required by applicable law or agreed to in writing, software
//  distributed under the License is distributed on an "AS IS" BASIS,
//  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
//  See the License for the specific language governing permissions and
//  limitations under the License.
// -------------------------------------------------------------------------------------------------

//! Testing helpers and fixtures for MT5 adapter.

#![cfg(test)]

use std::path::PathBuf;

/// Returns the path to the test_data directory.
///
/// # Examples
///
/// ```ignore
/// let dir = get_test_data_dir();
/// assert!(dir.ends_with("test_data"));
/// ```
pub fn get_test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data")
}

/// Loads a JSON fixture file from test_data directory.
///
/// # Arguments
///
/// * `filename` - Name of the file in test_data/ (e.g., "ws_tick_single.json")
///
/// # Returns
///
/// The file contents as a String.
///
/// # Panics
///
/// Panics if the file cannot be read or is not valid UTF-8.
///
/// # Examples
///
/// ```ignore
/// let json = load_test_json("ws_tick_single.json");
/// assert!(json.contains("BTCUSD"));
/// ```
pub fn load_test_json(filename: &str) -> String {
    let path = get_test_data_dir().join(filename);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "Failed to read test file {}: {}",
            path.display(),
            e
        )
    })
}

/// Loads and parses a JSON fixture as a specific type.
///
/// # Arguments
///
/// * `filename` - Name of the file in test_data/ (e.g., "ws_tick_single.json")
///
/// # Returns
///
/// The parsed JSON object of type T.
///
/// # Panics
///
/// Panics if the file cannot be parsed as the target type.
///
/// # Examples
///
/// ```ignore
/// use crate::websocket::messages::Mt5LiveTickMsg;
///
/// let msg: Mt5LiveTickMsg = load_test_json_as("ws_tick_single.json");
/// assert_eq!(msg.symbol, "BTCUSD");
/// ```
pub fn load_test_json_as<T: serde::de::DeserializeOwned>(filename: &str) -> T {
    let json = load_test_json(filename);
    serde_json::from_str(&json).unwrap_or_else(|e| {
        panic!(
            "Failed to parse {} as {}: {}",
            filename,
            std::any::type_name::<T>(),
            e
        )
    })
}

////////////////////////////////////////////////////////////////////////////////
// Tests
////////////////////////////////////////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_test_data_dir() {
        let dir = get_test_data_dir();
        assert!(dir.ends_with("test_data"));
        assert!(
            dir.exists(),
            "test_data directory does not exist at {}",
            dir.display()
        );
    }

    #[test]
    fn test_load_test_json_tick_single() {
        let json = load_test_json("ws_tick_single.json");
        assert!(json.contains("BTCUSD"));
        assert!(json.contains("TICK"));
        assert!(json.contains("CONNECTED"));
    }

    #[test]
    fn test_load_test_json_bar_single() {
        let json = load_test_json("ws_bar_m1_single.json");
        assert!(json.contains("XAUUSD"));
        assert!(json.contains("M1"));
    }

    #[test]
    fn test_load_test_json_account() {
        let json = load_test_json("http_get_account.json");
        assert!(json.contains("balance"));
        assert!(json.contains("equity"));
        assert!(json.contains("margin"));
    }

    #[test]
    fn test_load_test_json_as_value() {
        let value: serde_json::Value = load_test_json_as("ws_tick_single.json");
        assert_eq!(value["symbol"], "BTCUSD");
        assert_eq!(value["timeframe"], "TICK");
        assert_eq!(value["status"], "CONNECTED");
    }

    #[test]
    fn test_load_test_json_as_account() {
        let account: serde_json::Value = load_test_json_as("http_get_account.json");
        assert_eq!(account["error"], false);
        assert!(account["balance"].is_number());
        assert!(account["equity"].is_number());
    }

    #[test]
    #[should_panic(expected = "Failed to read test file")]
    fn test_load_nonexistent_file() {
        load_test_json("nonexistent_file.json");
    }

    #[test]
    #[should_panic(expected = "Failed to parse")]
    fn test_load_invalid_json_as_struct() {
        // Try to parse account data as an array (will fail)
        let _: Vec<String> = load_test_json_as("http_get_account.json");
    }
}
