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

# This example demonstrates a simple spread-based trading strategy for MT5.
# The strategy monitors bid/ask spreads and places market orders when spreads are tight.

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
from nautilus_trader.config import StrategyConfig
from nautilus_trader.config import TradingNodeConfig
from nautilus_trader.live.node import TradingNode
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TraderId
from nautilus_trader.trading.strategy import Strategy


class SpreadMonitorConfig(StrategyConfig):
    """Configuration for SpreadMonitor strategy."""

    instrument_id: str
    max_spread_pips: float = 2.0  # Maximum spread in pips
    trade_size: Decimal = Decimal("0.01")  # Trade size in lots
    max_positions: int = 1  # Maximum number of open positions


class SpreadMonitor(Strategy):
    """
    A simple strategy that monitors spreads and trades when spreads are tight.

    The strategy:
    - Subscribes to quote tick data
    - Monitors bid/ask spreads
    - Places market buy orders when spread is tight
    - Logs spread statistics
    """

    def __init__(self, config: SpreadMonitorConfig) -> None:
        super().__init__(config)
        self.instrument_id = InstrumentId.from_str(config.instrument_id)
        self.max_spread_pips = config.max_spread_pips
        self.trade_size = config.trade_size
        self.max_positions = config.max_positions

        # Statistics
        self.tick_count = 0
        self.min_spread = None
        self.max_spread = None
        self.total_spread = 0.0

    def on_start(self) -> None:
        """Actions to be performed on strategy start."""
        self.instrument = self.cache.instrument(self.instrument_id)
        if self.instrument is None:
            self.log.error(f"Could not find instrument for {self.instrument_id}")
            self.stop()
            return

        # Subscribe to quote ticks
        self.subscribe_quote_ticks(self.instrument_id)
        self.log.info(f"SpreadMonitor started for {self.instrument_id}")
        self.log.info(f"Max spread: {self.max_spread_pips} pips")
        self.log.info(f"Trade size: {self.trade_size} lots")

    def on_quote_tick(self, tick: QuoteTick) -> None:
        """
        Handle incoming quote ticks.

        Parameters
        ----------
        tick : QuoteTick
            The quote tick to process.

        """
        # Calculate spread in pips
        spread_raw = float(tick.ask_price - tick.bid_price)
        spread_pips = spread_raw / self.instrument.price_increment

        # Update statistics
        self.tick_count += 1
        self.total_spread += spread_pips

        if self.min_spread is None or spread_pips < self.min_spread:
            self.min_spread = spread_pips

        if self.max_spread is None or spread_pips > self.max_spread:
            self.max_spread = spread_pips

        # Log every 100 ticks
        if self.tick_count % 100 == 0:
            avg_spread = self.total_spread / self.tick_count
            self.log.info(
                f"Ticks: {self.tick_count} | "
                f"Current: {spread_pips:.1f} pips | "
                f"Avg: {avg_spread:.1f} | "
                f"Min: {self.min_spread:.1f} | "
                f"Max: {self.max_spread:.1f}"
            )

        # Trading logic: only trade if spread is tight
        if spread_pips <= self.max_spread_pips:
            # Check if we already have positions
            positions = self.cache.positions_open(
                venue=self.instrument_id.venue,
                instrument_id=self.instrument_id,
            )

            if len(positions) >= self.max_positions:
                self.log.debug(f"Already have {len(positions)} position(s), skipping trade")
                return

            # Place a market buy order
            self.log.info(
                f"Tight spread detected: {spread_pips:.1f} pips "
                f"(threshold: {self.max_spread_pips})"
            )

            # Uncomment the following line to enable actual trading
            # self._submit_market_order(OrderSide.BUY)

            self.log.warning("Trading disabled - uncomment order submission to enable")

    def _submit_market_order(self, side: OrderSide) -> None:
        """Submit a market order."""
        order = self.order_factory.market(
            instrument_id=self.instrument_id,
            order_side=side,
            quantity=self.instrument.make_qty(self.trade_size),
        )
        self.submit_order(order)
        self.log.info(f"Submitted {side} market order: {order.client_order_id}")

    def on_stop(self) -> None:
        """Actions to be performed when the strategy is stopped."""
        if self.tick_count > 0:
            avg_spread = self.total_spread / self.tick_count
            self.log.info("=" * 60)
            self.log.info("Spread Statistics:")
            self.log.info(f"  Total ticks: {self.tick_count}")
            self.log.info(f"  Average spread: {avg_spread:.2f} pips")
            self.log.info(f"  Min spread: {self.min_spread:.2f} pips")
            self.log.info(f"  Max spread: {self.max_spread:.2f} pips")
            self.log.info("=" * 60)


# Configuration
instrument_id = "EURUSD.MT5"  # Replace with your broker's symbol
mt5_account_id = "123456"  # Replace with your MT5 account number

# Configure the trading node
config_node = TradingNodeConfig(
    trader_id=TraderId("TESTER-001"),
    logging=LoggingConfig(log_level="INFO", use_pyo3=True),
    exec_engine=LiveExecEngineConfig(
        reconciliation=True,
    ),
    cache=CacheConfig(
        encoding="msgpack",
        timestamps_as_iso8601=True,
        buffer_interval_ms=100,
    ),
    data_clients={
        MT5: MT5DataClientConfig(
            host="localhost",
            live_port=2203,
            stream_port=2204,
            sys_port=2201,
            instrument_provider=InstrumentProviderConfig(load_all=True),
        ),
    },
    exec_clients={
        MT5: MT5ExecClientConfig(
            host="localhost",
            stream_port=2204,
            sys_port=2201,
            account_id=mt5_account_id,
            instrument_provider=InstrumentProviderConfig(load_all=True),
        ),
    },
    timeout_connection=60.0,
    timeout_reconciliation=20.0,
    timeout_portfolio=10.0,
    timeout_disconnection=5.0,
    timeout_post_stop=5.0,
)

# Instantiate the node
node = TradingNode(config=config_node)

# Configure the strategy
strategy_config = SpreadMonitorConfig(
    instrument_id=instrument_id,
    max_spread_pips=2.0,  # Only trade when spread <= 2 pips
    trade_size=Decimal("0.01"),  # 0.01 lots (micro lot)
    max_positions=1,  # Maximum 1 open position
)

# Instantiate the strategy
strategy = SpreadMonitor(config=strategy_config)

# Add strategy to trader
node.trader.add_strategy(strategy)

# Register client factories
node.add_data_client_factory(MT5, MT5LiveDataClientFactory)
node.add_exec_client_factory(MT5, MT5LiveExecClientFactory)
node.build()


# Stop and dispose of the node with SIGINT/CTRL+C
if __name__ == "__main__":
    try:
        node.run()
    finally:
        node.dispose()
