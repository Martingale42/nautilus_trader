# MT5 Adapter for NautilusTrader

MetaTrader 5 adapter using ZeroMQ to connect to MT5-ZeroMQ (JsonAPI.mq5).

## Architecture

This adapter follows the same patterns as other NautilusTrader adapters (Coinbase, Bybit) but uses native MT5 JSON format instead of emulating another exchange's protocol.

### Thread Model: Dedicated ZMQ Thread + Async Channels

```
NautilusTrader (Python/Async Rust)
    ↓ (async stream)
MT5 Adapter (Rust)
    ├─ Main Thread: PyO3 Bindings + Async API
    └─ ZMQ Thread: Dedicated OS thread for ZMQ sockets
        ├─ liveSocket (PULL) - tick/bar data (routes by timeframe field)
        ├─ streamSocket (PULL) - orders/positions
        ├─ dataSocket (PULL) - async responses (instruments, etc.)
        └─ sysSocket (REQ) - commands (subscribe, trade, query)
        ↓ (channel)
        MT5-ZeroMQ (JsonAPI.mq5)
            ↓
        MT5 Terminal
```

**Why This Design?**
- ZMQ sockets (`*mut c_void`) are not `Send` - can't cross thread boundaries
- Solution: Keep ZMQ in dedicated thread, use channels for communication
- Benefits: Async streaming + no Send/Sync issues + clean code

## Features

- Real-time quote tick streaming (bid/ask)
- Real-time bar streaming (M1 to MN1 timeframes)
- Historical bar data for reconciliation
- Order execution (market, limit, stop, stop-limit)
- Bracket orders with SL/TP (via MT5's native fields)
- Position and account state queries
- Execution reports (trade history)
- Native MT5 JSON format (no protocol emulation)

## Project Structure

```
mt5/
├── src/
│   ├── common/
│   │   ├── enums.rs          # MT5 enums (OrderType, TimeFrame, etc.)
│   │   ├── constants.rs      # MT5 error codes, retcodes
│   │   └── parse.rs          # Utility parsing functions
│   ├── websocket/            # ZeroMQ client (named for consistency)
│   │   ├── client.rs         # ZeroMQ subscriber client
│   │   ├── messages.rs       # MT5 native message structs
│   │   └── parse.rs          # MT5 → Nautilus converters
│   └── python/
│       ├── mod.rs            # PyO3 module registration
│       └── websocket.rs      # Python bindings
└── test_data/                # JSON fixtures from MT5
```

## MT5-ZeroMQ Connection

The adapter connects to three ZeroMQ sockets:

- **liveSocket** (PULL, port 2203): Live market data streaming (ticks and bars)
- **streamSocket** (PULL, port 2204): Order/position updates
- **sysSocket** (REQ, port 2201): Command/response (subscription, account queries, trade execution)
- **dataSocket** (PULL, port 2202): Async responses to commands (instruments, large datasets)

## Message Flow

### Quote Tick Data
1. Client subscribes via `subscribe_quotes([instrument_id])`
2. Adapter sends CONFIG message with `timeframe=TICK` to MT5 sysSocket
3. MT5 starts streaming tick data via liveSocket
4. Messages have format: `{"status": "CONNECTED", "symbol": "...", "timeframe": "TICK", "data": [timestamp_ms, bid, ask]}`
5. Adapter parses MT5 ticks → Nautilus QuoteTick
6. Python receives parsed Nautilus objects via callback

### Bar Data
1. Client subscribes via `subscribe_bars(bar_type)` (e.g., EURUSD-1-MINUTE-LAST)
2. Adapter sends CONFIG message with appropriate timeframe (M1, H1, etc.) to MT5 sysSocket
3. MT5 starts streaming bar data via liveSocket
4. Messages have format: `{"status": "CONNECTED", "symbol": "...", "timeframe": "M1", "data": [timestamp_sec, open, high, low, close, volume]}`
5. Adapter parses MT5 bars → Nautilus Bar objects
6. Python receives parsed Nautilus objects via callback

**Key Implementation Details**:
- The adapter stores a `(symbol, timeframe) -> bar_type_str` mapping when subscribing to bars
- This allows it to reconstruct the full BarType when parsing incoming bar messages (which only contain symbol and timeframe)
- Timestamps: Tick data uses **milliseconds**, bar data uses **seconds**
- The `handle_live_message()` function peeks at the `timeframe` field to route to the appropriate parser:
  - `timeframe == "TICK"` → `parse_mt5_live_tick_to_quote()`
  - `timeframe == "M1"|"H1"|etc.` → `parse_mt5_live_bar()`

### Order Execution
1. Client submits order via ExecutionClient
2. Adapter converts to MT5 TRADE format
3. Sends via sysSocket (REQ)
4. MT5 executes and responds
5. Order updates stream via streamSocket
6. Adapter parses → OrderStatusReport

## Development Status

**Overall Progress**: Live trading functional, under active testing

| Component | Status | LOC |
|-----------|--------|-----|
| Rust Core (ZeroMQ + PyO3) | ✅ Complete | ~4,000 |
| Python Adapter Layer | ✅ Complete | ~2,800 |
| **Total** | **Live trading ready** | **~6,800** |

### ✅ Implemented Features

**Data Client:**
- [x] Real-time quote tick streaming (bid/ask)
- [x] Real-time bar streaming (M1, M5, M15, M30, H1, H4, D1, W1, MN1)
- [x] Historical bar data retrieval for live trading reconciliation
- [x] Instrument loading from MT5

**Execution Client:**
- [x] Market orders
- [x] Limit orders
- [x] Stop orders (stop-market)
- [x] Stop-limit orders (mapped to MT5's native stop orders)
- [x] Bracket orders (entry + SL/TP) via MT5's native SL/TP fields
- [x] Order cancellation
- [x] Order modification
- [x] Position queries
- [x] Account state queries
- [x] Execution reports for live trading reconciliation (trade history)

**Architecture:**
- [x] Dedicated ZMQ thread + async channel pattern (handles non-Send ZMQ sockets)
- [x] PyO3 bindings with async support
- [x] Typed message variants following Bybit adapter pattern

### ⚠️ Known Limitations

- **Stop-limit orders**: MT5's stop orders are natively stop-limit. Both `StopMarket` and `StopLimit` map to `ORDER_TYPE_*_STOP`. The "stop limit price" field is not yet utilized for different trigger/limit prices.
- **Order events**: Bracket order SL/TP use virtual venue_order_ids (`{ticket}-SL`, `{ticket}-TP`). MT5 doesn't provide separate order IDs for attached SL/TP.
- **Shutdown timing**: Orders in `PENDING_CANCEL` state at shutdown may still exist on MT5 if confirmation wasn't received before disconnect.

### 📋 Todo
- [ ] Different trigger/limit prices for stop-limit orders
- [ ] Reconnection logic with order state recovery
- [ ] Stream socket order lifecycle events (partial fills, modifications)
- [ ] Integration test suite

## Building

```bash
# Build Rust crate
cargo build --release

# Build with Python bindings
cargo build --release --features python

# Run tests
cargo test
```

## Usage

```python
from nautilus_trader.adapters.mt5.data import MT5DataClient
from nautilus_trader.adapters.mt5.config import MT5DataClientConfig
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.data import BarType

# Create data client
config = MT5DataClientConfig(
    host="localhost",
    data_port=2202,
    live_port=2203,
    stream_port=2204,
    sys_port=2201,
)

client = MT5DataClient(
    loop=loop,
    msgbus=msgbus,
    cache=cache,
    clock=clock,
    config=config,
)

# Connect and start streaming
await client.connect()

# Subscribe to quote ticks
eurusd = InstrumentId.from_str("EURUSD.MT5")
await client.subscribe_quote_ticks(eurusd)

# Subscribe to bars (1-minute bars)
bar_type = BarType.from_str("EURUSD.MT5-1-MINUTE-LAST-EXTERNAL")
await client.subscribe_bars(bar_type)

# Subscribe to bars (1-hour bars)
bar_type_h1 = BarType.from_str("EURUSD.MT5-1-HOUR-LAST-EXTERNAL")
await client.subscribe_bars(bar_type_h1)

# Check status
if client.is_connected:
    print("Connected and streaming")

# Data will be published to the message bus automatically
# Subscribe to the message bus to receive QuoteTick and Bar objects

# Close when done
client.close()
```

### Supported Timeframes

The adapter supports the following MT5 timeframes for bar subscriptions:

- **M1** - 1 minute bars (BarAggregation.MINUTE with step=1)
- **M5** - 5 minute bars (BarAggregation.MINUTE with step=5)
- **M15** - 15 minute bars (BarAggregation.MINUTE with step=15)
- **M30** - 30 minute bars (BarAggregation.MINUTE with step=30)
- **H1** - 1 hour bars (BarAggregation.HOUR with step=1)
- **H4** - 4 hour bars (BarAggregation.HOUR with step=4)
- **D1** - Daily bars (BarAggregation.DAY)
- **W1** - Weekly bars (BarAggregation.WEEK)
- **MN1** - Monthly bars (BarAggregation.MONTH)

## Testing

Requires MT5-ZeroMQ (JsonAPI.mq5) running in MetaTrader 5 terminal.

```bash
# Unit tests (no MT5 required)
cargo test

# Integration tests (requires MT5)
cargo test --features integration-tests
```

## References

- [MT5-ZeroMQ](https://github.com/khramkov/MT5-ZeroMQ) - ZeroMQ bridge for MT5
- [NautilusTrader](https://nautilustrader.io/) - Algorithmic trading platform
- [Coinbase Adapter](../coinbase_intx/) - Reference implementation pattern
