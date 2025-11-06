# MetaTrader 5 Examples

This directory contains example scripts for using NautilusTrader with MetaTrader 5 (MT5).

## Prerequisites

Before running these examples, ensure you have:

1. **MT5 Terminal with ZeroMQ Bridge**: Running either:
   - Standalone MT5 with JsonAPI.mq5 Expert Advisor installed
   - MT5-Docker container (recommended)

2. **ZeroMQ Connectivity**: The following ports must be accessible:
   - `2203` - Live tick data streaming
   - `2204` - Order/position updates
   - `2201` - Commands and queries

3. **MT5 Account**: A valid MT5 account with your broker (demo or live)

4. **Instruments**: The symbols you want to trade must be visible in MT5 Market Watch

## Running the Docker Container

If using MT5-Docker (recommended approach):

```bash
# Clone the repository
git clone https://github.com/Martingale42/MT5-Docker.git
cd MT5-Docker

# Configure your MT5 credentials
cp .env.example .env
# Edit .env with your broker details:
#   MT5_LOGIN=your_account_number
#   MT5_PASSWORD=your_password
#   MT5_SERVER=your_broker_server

# Start the container
docker-compose up -d

# Check logs to verify connection
docker logs mt5-zeromq
```

## Examples Overview

### 1. Data Tester (`mt5_data_tester.py`)

Tests market data connectivity without placing any orders.

**What it does:**
- Connects to MT5 via ZeroMQ
- Loads instrument definitions
- Subscribes to quote ticks (bid/ask prices)
- Logs received data for verification

**Configuration:**
```python
instrument_ids = [
    InstrumentId.from_str("EURUSD.MT5"),
    InstrumentId.from_str("GBPUSD.MT5"),
    InstrumentId.from_str("USDJPY.MT5"),
]
```

**Run it:**
```bash
python mt5_data_tester.py
```

**Expected output:**
```
[INFO] Connected to MT5-ZeroMQ at localhost
[INFO] Subscribed to EURUSD.MT5 quote ticks
[INFO] QuoteTick(EURUSD.MT5, bid=1.08505, ask=1.08512, ...)
```

### 2. Execution Tester (`mt5_exec_tester.py`)

Tests order execution functionality with limit orders.

**What it does:**
- Uses the `ExecTester` strategy
- Places limit orders offset from market price
- Tests order lifecycle (submission, acceptance, fills, cancellations)
- Reconciles order state with MT5

**Configuration:**
```python
instrument_id = InstrumentId.from_str("EURUSD.MT5")
offset_ticks = 50  # Ticks away from market
trade_size = Decimal("0.01")  # 0.01 lots
mt5_account_id = "123456"  # Your MT5 account number
dry_run = True  # Set to False to enable actual trading
```

**⚠️ Important:**
- Default is `dry_run=True` (no actual orders placed)
- Set `dry_run=False` to enable real trading
- Use a demo account for testing first!

**Run it:**
```bash
python mt5_exec_tester.py
```

**Expected output (dry_run=True):**
```
[INFO] Connected to MT5-ZeroMQ at localhost
[INFO] ExecTester initialized (DRY RUN MODE)
[INFO] Would submit LIMIT BUY order at 1.08450 (50 ticks below market)
```

### 3. Simple Strategy (`mt5_simple_strategy.py`)

A practical example showing a custom spread-monitoring strategy.

**What it does:**
- Monitors bid/ask spreads in real-time
- Calculates spread statistics (min, max, average)
- Demonstrates conditional order placement logic
- Shows how to build a custom trading strategy

**Configuration:**
```python
strategy_config = SpreadMonitorConfig(
    instrument_id="EURUSD.MT5",
    max_spread_pips=2.0,  # Only trade when spread <= 2 pips
    trade_size=Decimal("0.01"),  # 0.01 lots
    max_positions=1,  # Maximum 1 open position
)
```

**⚠️ Trading is disabled by default:**
To enable actual trading, uncomment this line in `on_quote_tick()`:
```python
# self._submit_market_order(OrderSide.BUY)
```

**Run it:**
```bash
python mt5_simple_strategy.py
```

**Expected output:**
```
[INFO] SpreadMonitor started for EURUSD.MT5
[INFO] Max spread: 2.0 pips
[INFO] Ticks: 100 | Current: 1.2 pips | Avg: 1.5 | Min: 0.8 | Max: 3.2
[INFO] Tight spread detected: 1.8 pips (threshold: 2.0)
[WARN] Trading disabled - uncomment order submission to enable
```

## Customization Guide

### Changing Instruments

Update the instrument IDs to match your broker's symbols:

```python
# Check available symbols in MT5 Market Watch
instrument_ids = [
    InstrumentId.from_str("XAUUSD.MT5"),   # Gold
    InstrumentId.from_str("SPX500.MT5"),   # S&P 500 CFD
    InstrumentId.from_str("BTCUSD.MT5"),   # Bitcoin (if broker offers)
]
```

### Changing ZeroMQ Connection

If running MT5-Docker on a remote host:

```python
MT5DataClientConfig(
    host="192.168.1.100",  # Docker host IP
    live_port=2203,
    stream_port=2204,
    sys_port=2201,
)
```

### Enabling Database Persistence

Uncomment the database configuration to store data:

```python
from nautilus_trader.persistence.catalog import ParquetDataCatalog

cache=CacheConfig(
    database=DatabaseConfig(
        type="redis",  # or "postgres"
        host="localhost",
        port=6379,
    ),
)
```

### Adding Message Bus Streaming

Uncomment to enable Redis streaming:

```python
message_bus=MessageBusConfig(
    database=DatabaseConfig(
        type="redis",
        host="localhost",
        port=6379,
    ),
    encoding="json",
    streams_prefix="mt5",
    types_filter=[QuoteTick],  # Only stream quote ticks
)
```

## Troubleshooting

### No connection to MT5

**Error:**
```
[ERROR] Timeout waiting for connection after 30.0s
```

**Solutions:**
1. Verify MT5-Docker container is running: `docker ps`
2. Check JsonAPI.mq5 is active in MT5 Experts tab
3. Verify ports are not blocked by firewall
4. Check Docker logs: `docker logs mt5-zeromq`

### No market data received

**Error:**
```
[WARN] Subscribed to EURUSD.MT5 quote ticks
[No tick data appears]
```

**Solutions:**
1. Open MT5 Market Watch and ensure symbol is visible
2. Right-click Market Watch → Show All
3. Verify market is open (not weekend/holiday)
4. Check MT5 Experts tab for JsonAPI.mq5 logs

### Instrument not found

**Error:**
```
[ERROR] Could not find instrument for EURUSD.MT5
```

**Solutions:**
1. Verify symbol spelling matches MT5 exactly
2. Add symbol to Market Watch in MT5
3. Wait for instrument provider to refresh (default: 60 minutes)
4. Restart the script to reload instruments

### Order rejection

**Error:**
```
[ERROR] Order rejected: retcode=10019 (Insufficient funds)
```

**Common MT5 return codes:**
- `10006` - Request rejected (check permissions)
- `10018` - Market closed
- `10019` - Insufficient funds
- `10027` - Symbol not found

**Solutions:**
1. Verify sufficient account balance
2. Check trading is enabled in MT5 settings
3. Ensure market is open for trading
4. Review broker's margin requirements

## Safety Reminders

⚠️ **Before enabling live trading:**

1. **Test on demo account first** - Always test new strategies on demo accounts
2. **Verify dry_run setting** - Double-check `dry_run=True` until ready
3. **Start with small sizes** - Use micro lots (0.01) for initial testing
4. **Monitor closely** - Watch the first few trades carefully
5. **Set position limits** - Use `max_positions` to limit exposure
6. **Check broker fees** - Be aware of spreads, commissions, and overnight fees

## Further Information

- [MT5 Integration Guide](../../../docs/integrations/mt5.md)
- [MT5 API Reference](../../../docs/api_reference/adapters/mt5.md)
- [MT5-Docker Repository](https://github.com/Martingale42/MT5-Docker)
- [NautilusTrader Documentation](https://nautilustrader.io)

## Getting Help

If you encounter issues:

1. Check the troubleshooting section above
2. Review MT5-Docker logs: `docker logs mt5-zeromq`
3. Check MT5 Experts tab for JsonAPI.mq5 messages
4. Open an issue on [GitHub](https://github.com/nautechsystems/nautilus_trader/issues)
