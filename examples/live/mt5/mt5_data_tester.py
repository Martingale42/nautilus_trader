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

from nautilus_trader.adapters.mt5.config import MT5DataClientConfig
from nautilus_trader.adapters.mt5.constants import MT5
from nautilus_trader.adapters.mt5.factories import MT5LiveDataClientFactory
from nautilus_trader.cache.config import CacheConfig
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.config import LiveExecEngineConfig
from nautilus_trader.config import LoggingConfig
from nautilus_trader.config import TradingNodeConfig
from nautilus_trader.live.node import TradingNode
from nautilus_trader.model.data import BarType
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TraderId
from nautilus_trader.test_kit.strategies.tester_data import DataTester
from nautilus_trader.test_kit.strategies.tester_data import DataTesterConfig


# Configure the trading node
config_node = TradingNodeConfig(
    trader_id=TraderId("TESTER-001"),
    logging=LoggingConfig(log_level="INFO", use_pyo3=True),
    exec_engine=LiveExecEngineConfig(
        reconciliation=False,  # Not applicable for data-only testing
        inflight_check_interval_ms=0,  # Not applicable
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
            instrument_provider=InstrumentProviderConfig(load_all=True),
        ),
    },
    timeout_connection=30.0,
    timeout_reconciliation=10.0,  # Not applicable
    timeout_portfolio=10.0,
    timeout_disconnection=10.0,
    timeout_post_stop=0.0,  # Not required as no order state
)

# Instantiate the node with a configuration
node = TradingNode(config=config_node)


# Configure instruments to test
# Replace with symbols available in your MT5 Market Watch
instrument_ids = [
    InstrumentId.from_str("BTCUSD.MT5"),
    # InstrumentId.from_str("ADAUSD.MT5"),
    # InstrumentId.from_str("DOGEUSD.MT5"),
]

# Configure your actor
config_tester = DataTesterConfig(
    instrument_ids=instrument_ids,
    bar_types=[BarType.from_str("BTCUSD.MT5-1-MINUTE-LAST-EXTERNAL")], # , BarType.from_str("ADAUSD.MT5-1-MINUTE-LAST-EXTERNAL"), BarType.from_str("DOGEUSD.MT5-1-MINUTE-LAST-EXTERNAL")],
    subscribe_quotes=True,  # Subscribe to QuoteTicks (bid/ask)
    subscribe_trades=False,  # TradeTicks not commonly used for MT5
    subscribe_bars=True,  # Subscribe to BarTicks (M1)
    subscribe_instrument=True,
    subscribe_instrument_close=True,
    subscribe_instrument_status=True,
    # subscribe_book_deltas=False,  # Not supported by MT5
    # manage_book=False,  # Not supported by MT5
    # subscribe_book_at_interval=False,  # Not supported by MT5
    request_quotes=True,
    request_bars=True,
    log_data=True,  # Log received data for verification
    log_events=True,
    log_commands=True
)

# Instantiate your actor
data_tester = DataTester(config=config_tester)

# Add your actors and modules
node.trader.add_actor(data_tester)

# Register your client factories with the node (can take user-defined factories)
node.add_data_client_factory(MT5, MT5LiveDataClientFactory)
node.build()


# Stop and dispose of the node with SIGINT/CTRL+C
if __name__ == "__main__":
    try:
        node.run()
    finally:
        node.dispose()
