#!/usr/bin/env python3
# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2025 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
#  You may not use this file except in compliance with the License.
#  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
#
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#  See the License for the specific language governing permissions and
#  limitations under the License.
# -------------------------------------------------------------------------------------------------

# This example demonstrates using the ExecTester strategy for testing
# execution functionality with MetaTrader 5.
# The strategy places limit orders at a specified offset from the market.

from decimal import Decimal

from nautilus_trader.adapters.mt5.config import MT5DataClientConfig
from nautilus_trader.adapters.mt5.config import MT5ExecClientConfig
from nautilus_trader.adapters.mt5.constants import MT5
from nautilus_trader.adapters.mt5.factories import MT5LiveDataClientFactory
from nautilus_trader.adapters.mt5.factories import MT5LiveExecClientFactory
from nautilus_trader.cache.config import CacheConfig
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.config import LiveExecEngineConfig
from nautilus_trader.config import LoggingConfig
from nautilus_trader.config import TradingNodeConfig
from nautilus_trader.live.node import TradingNode
from nautilus_trader.model.enums import OrderType
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TraderId
from nautilus_trader.test_kit.strategies.tester_exec import ExecTester
from nautilus_trader.test_kit.strategies.tester_exec import ExecTesterConfig


# Test configuration
instrument_id: InstrumentId = InstrumentId.from_str("BTCUSD.MT5")  # Replace with your broker's symbol
offset_ticks: int = 50  # Number of ticks to offset limit orders from the market (wider for MT5)
trade_size: Decimal = Decimal("0.01")  # 0.01 lots (micro lot)
mt5_account_id = "1600039229"  # Replace with your MT5 account number (just the number, MT5 prefix added automatically)
dry_run = False  # Set this to False to enable actual trading (CAUTION!)

# Configure the trading node
config_node = TradingNodeConfig(
    trader_id=TraderId("TESTER-001"),
    logging=LoggingConfig(log_level="INFO", use_pyo3=True),
    exec_engine=LiveExecEngineConfig(
        reconciliation=True,  # Enable reconciliation for order state sync
        # snapshot_orders=True,
        # snapshot_positions=True,
        # snapshot_positions_interval_secs=5.0,
    ),
    cache=CacheConfig(
        # database=DatabaseConfig(),
        encoding="msgpack",
        timestamps_as_iso8601=True,
        buffer_interval_ms=100,
    ),
    # message_bus=MessageBusConfig(
    #     database=DatabaseConfig(),
    #     encoding="json",
    #     timestamps_as_iso8601=True,
    #     buffer_interval_ms=100,
    #     streams_prefix="mt5",
    #     use_instance_id=False,
    #     # types_filter=[QuoteTick],
    #     autotrim_mins=30,
    # ),
    # heartbeat_interval=1.0,
    data_clients={
        MT5: MT5DataClientConfig(
            host="localhost",  # ZeroMQ host (use Docker container IP if remote)
            data_port=2202,  # Command responses port
            live_port=2203,  # Tick data streaming port
            stream_port=2204,  # Order/position updates port
            sys_port=2201,  # Commands/queries port
            account_id=mt5_account_id,  # IMPORTANT: Must match exec client for shared Mt5Client instance
            instrument_provider=InstrumentProviderConfig(load_all=True),
        ),
    },
    exec_clients={
        MT5: MT5ExecClientConfig(
            host="localhost",  # ZeroMQ host
            stream_port=2204,  # Order/position updates port
            sys_port=2201,  # Commands/queries port
            account_id=mt5_account_id,  # Your MT5 account number
            instrument_provider=InstrumentProviderConfig(load_all=True),
        ),
    },
    timeout_connection=60.0,
    timeout_reconciliation=20.0,
    timeout_portfolio=10.0,
    timeout_disconnection=5.0,
    timeout_post_stop=5.0,
)

# Instantiate the node with a configuration
node = TradingNode(config=config_node)

# Configure your strategy
# Order types tested:
# - LIMIT (buy/sell) via enable_buys/enable_sells (default True)
# - STOP_LIMIT via stop_order_type + enable_stop_buys/enable_stop_sells
# - BRACKET orders via enable_brackets (LIMIT entry + SL/TP)
# Change stop_order_type to OrderType.STOP_MARKET to test stop market orders instead
config_tester = ExecTesterConfig(
    instrument_id=instrument_id,
    external_order_claims=[instrument_id],  # Claim external orders for this instrument
    order_qty=trade_size,
    tob_offset_ticks=offset_ticks,
    subscribe_quotes=True,  # Subscribe to QuoteTicks for market data
    subscribe_trades=False,  # Not commonly used for MT5
    enable_stop_buys=True,
    enable_stop_sells=True,
    stop_order_type=OrderType.STOP_LIMIT,  # Test stop-limit orders (trigger + limit price)
    enable_brackets=True,
    use_post_only=False,  # MT5 doesn't support post_only
    close_positions_time_in_force=TimeInForce.GTC,  # MT5 default time-in-force
    dry_run=dry_run,  # Set to False to enable actual trading
    log_data=True,
)

# Instantiate your strategy
strategy = ExecTester(config=config_tester)

# Add your strategies and modules
node.trader.add_strategy(strategy)

# Register your client factories with the node (can take user-defined factories)
node.add_data_client_factory(MT5, MT5LiveDataClientFactory)
node.add_exec_client_factory(MT5, MT5LiveExecClientFactory)
node.build()


# Stop and dispose of the node with SIGINT/CTRL+C
if __name__ == "__main__":
    try:
        node.run()
    finally:
        node.dispose()
