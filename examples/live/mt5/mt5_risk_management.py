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

# This example demonstrates risk management with stop loss and take profit orders for MT5.
# Shows how to place protective orders and manage position risk.

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
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.enums import TriggerType
from nautilus_trader.model.events import OrderFilled
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TraderId
from nautilus_trader.model.objects import Price
from nautilus_trader.model.objects import Quantity
from nautilus_trader.trading.strategy import Strategy


class RiskManagedStrategyConfig(StrategyConfig):
    """Configuration for risk-managed strategy."""

    instrument_id: str
    trade_size: Decimal = Decimal("0.01")
    stop_loss_pips: float = 20.0  # Stop loss in pips
    take_profit_pips: float = 40.0  # Take profit in pips (2:1 risk/reward)
    max_positions: int = 1
    enable_trading: bool = False  # Safety: disabled by default


class RiskManagedStrategy(Strategy):
    """
    A strategy demonstrating proper risk management with MT5.

    Features:
    - Stop loss orders for downside protection
    - Take profit orders for profit targets
    - Position sizing
    - Risk/reward ratio management
    """

    def __init__(self, config: RiskManagedStrategyConfig) -> None:
        super().__init__(config)
        self.instrument_id = InstrumentId.from_str(config.instrument_id)
        self.trade_size = config.trade_size
        self.stop_loss_pips = config.stop_loss_pips
        self.take_profit_pips = config.take_profit_pips
        self.max_positions = config.max_positions
        self.enable_trading = config.enable_trading

        # Track pending protective orders
        self._pending_stop_loss = {}  # {client_order_id: (stop_price, quantity)}
        self._pending_take_profit = {}  # {client_order_id: (tp_price, quantity)}

    def on_start(self) -> None:
        """Actions to be performed on strategy start."""
        self.instrument = self.cache.instrument(self.instrument_id)
        if self.instrument is None:
            self.log.error(f"Could not find instrument for {self.instrument_id}")
            self.stop()
            return

        # Subscribe to quote ticks
        self.subscribe_quote_ticks(self.instrument_id)

        self.log.info("=" * 60)
        self.log.info("Risk Managed Strategy Started")
        self.log.info(f"Instrument: {self.instrument_id}")
        self.log.info(f"Trade size: {self.trade_size} lots")
        self.log.info(f"Stop loss: {self.stop_loss_pips} pips")
        self.log.info(f"Take profit: {self.take_profit_pips} pips")
        self.log.info(f"Risk/Reward: 1:{self.take_profit_pips / self.stop_loss_pips:.1f}")
        self.log.info(f"Trading enabled: {self.enable_trading}")
        self.log.info("=" * 60)

    def on_quote_tick(self, tick: QuoteTick) -> None:
        """
        Handle incoming quote ticks.

        Parameters
        ----------
        tick : QuoteTick
            The quote tick to process.

        """
        # Check if we already have positions
        positions = self.cache.positions_open(
            venue=self.instrument_id.venue,
            instrument_id=self.instrument_id,
        )

        if len(positions) >= self.max_positions:
            return  # Already at max positions

        # Simple entry logic: enter on every 100th tick (for demonstration)
        # In a real strategy, you would use technical indicators, signals, etc.
        if not hasattr(self, "_tick_counter"):
            self._tick_counter = 0

        self._tick_counter += 1

        if self._tick_counter % 100 == 0:
            # Demonstrate a BUY entry with stop loss and take profit
            self._enter_long(tick)

    def _enter_long(self, tick: QuoteTick) -> None:
        """
        Enter a long position with stop loss and take profit.

        Parameters
        ----------
        tick : QuoteTick
            The current quote tick.

        """
        # Calculate entry price (use ask price for buying)
        entry_price = tick.ask_price

        # Calculate stop loss price (below entry)
        stop_loss_distance = self.stop_loss_pips * self.instrument.price_increment
        stop_loss_price = Price(float(entry_price) - stop_loss_distance, self.instrument.price_precision)

        # Calculate take profit price (above entry)
        take_profit_distance = self.take_profit_pips * self.instrument.price_increment
        take_profit_price = Price(float(entry_price) + take_profit_distance, self.instrument.price_precision)

        # Log the trade setup
        self.log.info("=" * 60)
        self.log.info("TRADE SETUP - LONG ENTRY")
        self.log.info(f"Entry Price: {entry_price}")
        self.log.info(f"Stop Loss:   {stop_loss_price} (-{self.stop_loss_pips} pips)")
        self.log.info(f"Take Profit: {take_profit_price} (+{self.take_profit_pips} pips)")

        # Calculate risk in account currency
        # risk_per_lot = stop_loss_pips * pip_value
        # For EURUSD, 1 pip = $10 for 1 lot, so 0.01 lot = $0.10 per pip
        pip_value_micro_lot = 0.10  # Approximate for EURUSD micro lot
        risk_amount = self.stop_loss_pips * pip_value_micro_lot * float(self.trade_size) / 0.01
        potential_profit = self.take_profit_pips * pip_value_micro_lot * float(self.trade_size) / 0.01

        self.log.info(f"Risk Amount: ${risk_amount:.2f}")
        self.log.info(f"Potential Profit: ${potential_profit:.2f}")
        self.log.info("=" * 60)

        if not self.enable_trading:
            self.log.warning("Trading disabled - set enable_trading=True to place actual orders")
            return

        # Create market order with stop loss and take profit
        # NOTE: MT5 supports attaching SL/TP to market orders
        order = self.order_factory.market(
            instrument_id=self.instrument_id,
            order_side=OrderSide.BUY,
            quantity=self.instrument.make_qty(self.trade_size),
            time_in_force=TimeInForce.GTC,
        )

        # Submit the order
        self.submit_order(order)
        self.log.info(f"Submitted market order: {order.client_order_id}")

        # Store protective order parameters for submission after fill
        # In MT5, SL/TP should be attached after position opens
        self._pending_stop_loss[order.client_order_id] = (stop_loss_price, self.instrument.make_qty(self.trade_size))
        self._pending_take_profit[order.client_order_id] = (take_profit_price, self.instrument.make_qty(self.trade_size))

    def on_order_filled(self, event: OrderFilled) -> None:
        """
        Handle order filled events to attach protective orders.

        Parameters
        ----------
        event : OrderFilled
            The order filled event.

        """
        self.log.info(f"Order filled: {event.client_order_id} at {event.last_px}")

        # Check if we have pending protective orders for this fill
        if event.client_order_id not in self._pending_stop_loss:
            return  # No protective orders pending for this order

        # Get the protective order parameters
        stop_loss_price, quantity = self._pending_stop_loss.pop(event.client_order_id)
        take_profit_price, _ = self._pending_take_profit.pop(event.client_order_id)

        # Determine the side for protective orders (opposite of entry for closing)
        protective_side = OrderSide.SELL if event.order_side == OrderSide.BUY else OrderSide.BUY

        # Submit stop loss order (STOP_MARKET)
        stop_loss_order = self.order_factory.stop_market(
            instrument_id=self.instrument_id,
            order_side=protective_side,
            quantity=quantity,
            trigger_price=stop_loss_price,
            trigger_type=TriggerType.DEFAULT,
            time_in_force=TimeInForce.GTC,
        )
        self.submit_order(stop_loss_order)
        self.log.info(f"Submitted stop loss order at {stop_loss_price}")

        # Submit take profit order (LIMIT)
        take_profit_order = self.order_factory.limit(
            instrument_id=self.instrument_id,
            order_side=protective_side,
            quantity=quantity,
            price=take_profit_price,
            time_in_force=TimeInForce.GTC,
        )
        self.submit_order(take_profit_order)
        self.log.info(f"Submitted take profit order at {take_profit_price}")

        self.log.info("=" * 60)
        self.log.info("PROTECTIVE ORDERS PLACED")
        self.log.info(f"Stop Loss: {stop_loss_order.client_order_id}")
        self.log.info(f"Take Profit: {take_profit_order.client_order_id}")
        self.log.info("=" * 60)

    def on_position_opened(self, position) -> None:
        """Handle position opened events."""
        self.log.info(f"Position opened: {position}")

    def on_position_closed(self, position) -> None:
        """Handle position closed events."""
        realized_pnl = position.realized_pnl
        self.log.info("=" * 60)
        self.log.info(f"Position closed: {position.id}")
        self.log.info(f"Realized P&L: {realized_pnl}")
        self.log.info("=" * 60)

    def on_stop(self) -> None:
        """Actions to be performed when the strategy is stopped."""
        self.log.info("Risk Managed Strategy stopped")


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
strategy_config = RiskManagedStrategyConfig(
    instrument_id=instrument_id,
    trade_size=Decimal("0.01"),  # 0.01 lots (micro lot)
    stop_loss_pips=20.0,  # 20 pip stop loss
    take_profit_pips=40.0,  # 40 pip take profit (2:1 R:R)
    max_positions=1,
    enable_trading=False,  # Set to True to enable actual trading
)

# Instantiate the strategy
strategy = RiskManagedStrategy(config=strategy_config)

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
