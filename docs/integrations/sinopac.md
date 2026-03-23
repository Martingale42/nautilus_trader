# Sinopac

[SinoPac Securities](https://www.sinotrade.com.tw/) (永豐金證券) is a major Taiwanese
brokerage providing access to stocks (TWSE/TPEX), futures, and options (TAIFEX).

This integration supports live market data ingest and order execution through a
self-hosted FastAPI gateway that bridges the [Shioaji](https://sinotrade.github.io/)
Python SDK.

## Overview

This adapter is implemented in Rust with Python bindings. It connects to a
self-hosted gateway rather than directly to the exchange, keeping the Shioaji
Python SDK dependency isolated from the Rust core.

The Sinopac adapter includes multiple components:

- `SinopacHttpClient`: Low-level HTTP API connectivity to the gateway.
- `SinopacWebSocketClient`: Low-level WebSocket API connectivity for streaming data.
- `SinopacInstrumentProvider`: Instrument parsing and loading functionality.
- `SinopacDataClient`: Market data feed manager.
- `SinopacExecutionClient`: Account management and trade execution gateway.
- `SinopacLiveDataClientFactory`: Factory for Sinopac data clients (used by the trading node builder).
- `SinopacLiveExecClientFactory`: Factory for Sinopac execution clients (used by the trading node builder).

:::note
Most users will define a configuration for a live trading node (as shown below)
and won't need to work directly with these lower-level components.
:::

## Examples

You can find live example scripts [here](https://github.com/nautechsystems/nautilus_trader/tree/develop/examples/live/sinopac/).

## Gateway setup

The Sinopac adapter requires a self-hosted FastAPI gateway
([shioaji-server](https://github.com/Martingale42/shioaji-server)) that translates
between NautilusTrader's Rust networking layer and the Shioaji Python SDK. The
gateway exposes typed REST and WebSocket endpoints for market data and order execution.

:::warning
The gateway must be running before starting the trading node. The adapter connects
to `http://localhost:8000` by default.
:::

## Product support

| Product Type | Data Feed | Trading | Notes                                    |
|--------------|-----------|---------|------------------------------------------|
| Stocks       | ✓         | ✓       | TWSE and TPEX listed equities.           |
| Futures      | ✓         | ✓       | TAIFEX index and commodity futures.      |
| Options      | ✓         | ✓       | TAIFEX index options.                    |

## Symbology

Instruments use native Sinopac contract codes as symbols:

### Stocks

Format: `{code}` (numeric stock code)

Examples:

- `2330` — TSMC
- `2317` — Hon Hai Precision

To subscribe in your strategy:

```python
InstrumentId.from_str("2330.SINOPAC")
InstrumentId.from_str("2317.SINOPAC")
```

### Futures

Format: `{root}{delivery}` (product root + delivery month/year code)

Examples:

- `TXFC6` — TAIEX futures, March 2026
- `MXFC6` — Mini-TAIEX futures, March 2026

```python
InstrumentId.from_str("TXFC6.SINOPAC")
```

### Options

Format: `{root}{delivery}{strike}{type}` (product root + delivery + strike + C/P)

```python
InstrumentId.from_str("TXO19000C6.SINOPAC")
```

## Data subscriptions

| Data type         | Subscription | Historical | Nautilus type        | Notes                              |
|-------------------|--------------|------------|----------------------|------------------------------------|
| Trade ticks       | ✓            | ✓          | `TradeTick`          | Via WebSocket tick stream.         |
| Quote ticks       | ✓            | -          | `QuoteTick`          | Top-of-book from BidAsk stream.   |
| Order book depth  | ✓            | -          | `OrderBookDepth10`   | 5-level depth from BidAsk stream. |
| Order book deltas | ✓            | -          | `OrderBookDeltas`    | CLEAR + ADD snapshot pattern.     |
| Bars              | -            | ✓          | `Bar`                | Historical only via REST.         |

:::note
Quote ticks, order book depth, and order book deltas all derive from the same
BidAsk WebSocket stream. The adapter only parses and delivers the data types with
active subscriptions, using per-instrument emit flags for efficiency.
:::

## Order capability

### Order types

| Order Type | Stocks | Futures | Options | Notes |
|------------|--------|---------|---------|-------|
| `MARKET`   | ✓      | ✓       | ✓       |       |
| `LIMIT`    | ✓      | ✓       | ✓       |       |

### Time in force

| Time in force | Stocks | Futures | Options | Notes               |
|---------------|--------|---------|---------|---------------------|
| `DAY`         | ✓      | ✓       | ✓       | Rest of day (ROD).  |
| `IOC`         | ✓      | ✓       | ✓       | Immediate or cancel.|
| `FOK`         | ✓      | ✓       | ✓       | Fill or kill.       |

### Order operations

| Operation    | Stocks | Futures | Options | Notes                         |
|--------------|--------|---------|---------|-------------------------------|
| Submit order | ✓      | ✓       | ✓       |                               |
| Modify order | ✓      | ✓       | ✓       | Price and quantity.            |
| Cancel order | ✓      | ✓       | ✓       |                               |

## Order books

Order books are maintained via the BidAsk WebSocket stream. Each message delivers
a 5-level snapshot. The adapter supports two consumption patterns:

- **`OrderBookDepth10`**: Direct 5-level snapshot with bid/ask arrays.
- **`OrderBookDeltas`**: CLEAR + ADD pattern for incremental book maintenance.

:::note
There is a limitation of one order book per instrument per trader instance.
:::

## Connection management

The adapter automatically reconnects on WebSocket disconnection using exponential
backoff (starting at 500ms, up to 5s). On reconnect, all active subscriptions are
resubscribed automatically. A heartbeat ping is sent every 30 seconds.

## Configuration

### Data client configuration options

| Option           | Default       | Description                               |
|------------------|---------------|-------------------------------------------|
| `venue`          | `"SINOPAC"`   | Venue identifier.                         |
| `gateway_host`   | `"localhost"` | Gateway host address.                     |
| `gateway_port`   | `8000`        | Gateway HTTP/WS port.                     |
| `gateway_ws_path`| `"/ws"`       | WebSocket endpoint path on the gateway.   |

### Execution client configuration options

| Option           | Default       | Description                                                                 |
|------------------|---------------|-----------------------------------------------------------------------------|
| `venue`          | `"SINOPAC"`   | Venue identifier.                                                           |
| `account_id`     | `None`        | Sinopac account identifier. Loaded from `SINOPAC_ACCOUNT_ID` when omitted.  |
| `gateway_host`   | `"localhost"` | Gateway host address.                                                       |
| `gateway_port`   | `8000`        | Gateway HTTP/WS port.                                                       |
| `gateway_ws_path`| `"/ws"`       | WebSocket endpoint path on the gateway.                                     |

### Configuration example

```python
from nautilus_trader.adapters.sinopac import SINOPAC
from nautilus_trader.adapters.sinopac import SinopacDataClientConfig
from nautilus_trader.adapters.sinopac import SinopacExecClientConfig
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.config import TradingNodeConfig

config = TradingNodeConfig(
    data_clients={
        SINOPAC: SinopacDataClientConfig(
            instrument_provider=InstrumentProviderConfig(load_all=True),
            gateway_host="localhost",
            gateway_port=8000,
        ),
    },
    exec_clients={
        SINOPAC: SinopacExecClientConfig(
            instrument_provider=InstrumentProviderConfig(load_all=True),
            account_id=None,  # Loads from SINOPAC_ACCOUNT_ID env var
            gateway_host="localhost",
            gateway_port=8000,
        ),
    },
)
```

Then, create a `TradingNode` and add the client factories:

```python
from nautilus_trader.adapters.sinopac import SINOPAC
from nautilus_trader.adapters.sinopac import SinopacLiveDataClientFactory
from nautilus_trader.adapters.sinopac import SinopacLiveExecClientFactory
from nautilus_trader.live.node import TradingNode

# Instantiate the live trading node with a configuration
node = TradingNode(config=config)

# Register the client factories with the node
node.add_data_client_factory(SINOPAC, SinopacLiveDataClientFactory)
node.add_exec_client_factory(SINOPAC, SinopacLiveExecClientFactory)

# Finally build the node
node.build()
```

## API credentials

Set the following environment variable for execution client authentication:

- `SINOPAC_ACCOUNT_ID`: Your Sinopac brokerage account identifier.

:::tip
We recommend using environment variables to manage your credentials.
:::

:::note
API key and secret for the Shioaji SDK are configured on the gateway side,
not in the NautilusTrader adapter configuration.
:::

## Contributing

:::info
For additional features or to contribute to the Sinopac adapter, please see our
[contributing guide](https://github.com/nautechsystems/nautilus_trader/blob/develop/CONTRIBUTING.md).
:::
