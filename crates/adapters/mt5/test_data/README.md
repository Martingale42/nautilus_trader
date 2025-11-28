# MT5 Adapter Test Data

This directory contains JSON fixtures captured from real MT5-ZeroMQ responses for use in unit and integration tests.

## File Naming Convention

- `ws_*.json` - WebSocket/streaming messages (from liveSocket/streamSocket)
- `http_*.json` - Request/response messages (from sysSocket/dataSocket)
- `*_single.json` - Single message for fast unit tests
- `*_minimal.json` - Minimal dataset (2-3 items) for fast tests
- `*_empty.json` - Empty response for edge case testing

## Files

### WebSocket Streaming Data

| File | Description | Source Socket | Count |
|------|-------------|---------------|-------|
| `ws_tick.json` | Live tick data stream (BTCUSD) | liveSocket | ~17 ticks |
| `ws_tick_single.json` | Single tick message | - | 1 tick |
| `ws_bar_m1.json` | Live M1 bar data (XAUUSD.sml) | liveSocket | 1 bar |
| `ws_bar_m1_single.json` | Single M1 bar message | - | 1 bar |

**Tick Format**: `{"status": "CONNECTED", "symbol": "...", "timeframe": "TICK", "data": [timestamp_ms, bid, ask]}`

**Bar Format**: `{"status": "CONNECTED", "symbol": "...", "timeframe": "M1", "data": [timestamp_sec, open, high, low, close, volume]}`

### Command Responses (Data Socket)

| File | Description | Action | Count |
|------|-------------|--------|-------|
| `http_get_instruments.json` | All available instruments | SYMBOL_INFO | ~138 symbols |
| `http_get_instruments_minimal.json` | Curated minimal instrument list | - | 2 symbols |
| `http_get_account.json` | Account state (balance, equity, margin) | ACCOUNT | 1 account |
| `http_get_positions.json` | All open positions | POSITIONS | ~44 positions |
| `http_get_positions_single.json` | Single position | - | 1 position |
| `http_get_positions_empty.json` | No open positions | - | 0 positions |
| `http_get_orders.json` | Pending orders | ORDERS | 0 orders |
| `http_get_history_bars.json` | Historical bar data | HISTORY (bars) | ~10,080 bars |
| `http_get_history_ticks.json` | Historical tick data | HISTORY (ticks) | ~3,600 ticks |

### Configuration/Subscription Commands

| File | Description | Action | Result |
|------|-------------|--------|--------|
| `http_post_config_tick.json` | Subscribe to tick data | CONFIG (TICK) | ACK |
| `http_post_config_m1.json` | Subscribe to M1 bars | CONFIG (M1) | ACK |

### Error Responses

| File | Description |
|------|-------------|
| `http_error_response.json` | Generic error response (ERR_WRONG_ACTION) |

## Data Sources

All fixtures were captured from MT5-ZeroMQ (JsonAPI.mq5) running against an OANDA demo account.

**Capture Date**: 2025-11-26
**MT5 Bridge**: [MT5-ZeroMQ](https://github.com/khramkov/MT5-ZeroMQ)
**Capture Script**: `MT5-Docker/scripts/capture_responses.py`

## Usage in Tests

```rust
use crate::common::testing::load_test_json;

#[test]
fn test_parse_tick() {
    let json = load_test_json("ws_tick_single.json");
    let msg: Mt5LiveTickMsg = serde_json::from_str(&json).unwrap();
    assert_eq!(msg.symbol, "BTCUSD");
}
```

## Updating Fixtures

To capture fresh data from your MT5 instance:

```bash
cd ~/Code/MT5/MT5-Docker
python3 scripts/capture_responses.py
# Copy updated files to this directory
```

## Notes

- **Timestamps**: Tick data uses milliseconds, bar data uses seconds
- **Symbols**: Some symbols have `.sml` suffix (e.g., `XAUUSD.sml`)
- **Empty Responses**: Some actions return minimal JSON (e.g., `{"error": false}`)
- **Position IDs**: Position IDs are unique integers assigned by MT5
- **Time Format**: All timestamps are Unix epoch (seconds or milliseconds)
