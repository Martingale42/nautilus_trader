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

from __future__ import annotations

import asyncio
from typing import Any

from nautilus_trader.adapters.env import get_env_key
from nautilus_trader.adapters.mt5.config import MT5ExecClientConfig
from nautilus_trader.adapters.mt5.constants import MT5
from nautilus_trader.adapters.mt5.constants import MT5_SUPPORTED_ORDER_TYPES
from nautilus_trader.adapters.mt5.constants import MT5_SUPPORTED_TIF
from nautilus_trader.adapters.mt5.constants import MT5_VENUE
from nautilus_trader.adapters.mt5.providers import MT5InstrumentProvider
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.common.enums import LogColor
from nautilus_trader.common.enums import LogLevel
from nautilus_trader.core import nautilus_pyo3
from nautilus_trader.core.correctness import PyCondition
from nautilus_trader.execution.messages import CancelAllOrders
from nautilus_trader.execution.messages import CancelOrder
from nautilus_trader.execution.messages import GenerateFillReports
from nautilus_trader.execution.messages import GenerateOrderStatusReport
from nautilus_trader.execution.messages import GenerateOrderStatusReports
from nautilus_trader.execution.messages import GeneratePositionStatusReports
from nautilus_trader.execution.messages import QueryAccount
from nautilus_trader.execution.messages import SubmitOrder
from nautilus_trader.execution.reports import FillReport
from nautilus_trader.execution.reports import OrderStatusReport
from nautilus_trader.execution.reports import PositionStatusReport
from nautilus_trader.live.cancellation import DEFAULT_FUTURE_CANCELLATION_TIMEOUT
from nautilus_trader.live.cancellation import cancel_tasks_with_timeout
from nautilus_trader.live.execution_client import LiveExecutionClient
from nautilus_trader.model.enums import AccountType
from nautilus_trader.model.enums import OmsType
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.enums import OrderType
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.functions import order_side_to_pyo3
from nautilus_trader.model.functions import time_in_force_to_pyo3
from nautilus_trader.model.identifiers import AccountId
from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import ClientOrderId
from nautilus_trader.model.identifiers import VenueOrderId
from nautilus_trader.model.orders import LimitOrder
from nautilus_trader.model.orders import MarketOrder
from nautilus_trader.model.orders import Order
from nautilus_trader.model.orders import StopLimitOrder
from nautilus_trader.model.orders import StopMarketOrder
from nautilus_trader.model.position import Position
from nautilus_trader.model.currencies import Currency


class _Mt5OrderReport:
    """
    Simple wrapper for MT5 order response containing venue_order_id.

    This is used to pass the MT5 ticket number to the order accepted event.
    """
    def __init__(self, venue_order_id_value: str):
        self.venue_order_id = type('VenueOrderId', (), {'value': venue_order_id_value})()


class MT5ExecutionClient(LiveExecutionClient):
    """
    Provides an execution client for the MetaTrader 5 trading platform.

    Parameters
    ----------
    loop : asyncio.AbstractEventLoop
        The event loop for the client.
    client : nautilus_pyo3.Mt5Client
        The MT5 ZeroMQ client.
    msgbus : MessageBus
        The message bus for the client.
    cache : Cache
        The cache for the client.
    clock : LiveClock
        The clock for the client.
    instrument_provider : MT5InstrumentProvider
        The instrument provider.
    config : MT5ExecClientConfig
        The configuration for the client.
    name : str, optional
        The custom client ID.

    """

    def __init__(
        self,
        loop: asyncio.AbstractEventLoop,
        client: nautilus_pyo3.Mt5Client,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
        instrument_provider: MT5InstrumentProvider,
        config: MT5ExecClientConfig,
        name: str | None,
    ) -> None:
        super().__init__(
            loop=loop,
            client_id=ClientId(name or MT5),
            venue=MT5_VENUE,
            oms_type=OmsType.NETTING,  # MT5 uses netting (not hedging by default)
            instrument_provider=instrument_provider,
            account_type=AccountType.MARGIN,  # MT5 is margin-based
            base_currency=None,  # Multi-currency
            msgbus=msgbus,
            cache=cache,
            clock=clock,
        )
        self._instrument_provider: MT5InstrumentProvider = instrument_provider

        # Configuration
        self._config = config
        self._log.info(f"Host: {config.host}:{config.sys_port}", LogColor.BLUE)

        # MT5 account
        self._account_id_str: str = config.account_id or get_env_key("MT5_ACCOUNT_ID")
        account_id = AccountId(f"{name or MT5}-{self._account_id_str}")
        self._set_account_id(account_id)
        self.pyo3_account_id = nautilus_pyo3.AccountId(account_id.value)
        self._log.info(f"account_id={self.account_id.value}", LogColor.BLUE)

        # ZeroMQ client
        self._client = client
        self._client_futures: set[asyncio.Future] = set()

    @property
    def mt5_instrument_provider(self) -> MT5InstrumentProvider:
        return self._instrument_provider

    async def _connect(self) -> None:
        """
        Connect to MT5 and synchronize account state.

        """
        await self._cache_instruments()
        await self._update_account_state()
        await self._await_account_registered()

        self._log.info("MT5 API authenticated", LogColor.GREEN)

        # Connect to stream socket for order/position updates
        # Note: This would be a separate connection in the Rust client
        # For now, we assume the data client handles the stream
        self._log.info("Connected to MT5 order stream", LogColor.BLUE)

    async def _disconnect(self) -> None:
        """
        Disconnect from MT5.

        """
        # Cancel any pending futures
        await cancel_tasks_with_timeout(
            self._client_futures,
            self._log,
            timeout_secs=DEFAULT_FUTURE_CANCELLATION_TIMEOUT,
        )
        self._client_futures.clear()

        self._log.info("Disconnected from MT5", LogColor.BLUE)

    async def _cache_instruments(self) -> None:
        """
        Cache instruments for correct parsing.

        """
        await self._instrument_provider.initialize()

        instruments_pyo3 = self.mt5_instrument_provider.instruments_pyo3()
        for inst in instruments_pyo3:
            self._client.add_instrument(inst)

        self._log.debug(f"Cached {len(instruments_pyo3)} instruments", LogColor.MAGENTA)

    async def _update_account_state(self) -> None:
        """
        Query and update account state from MT5.

        """
        try:
            # Query account state from MT5
            account_data = await self._client.request_account_state()

            # Extract account info
            balance = account_data.get("balance", 0.0)
            equity = account_data.get("equity", 0.0)
            margin = account_data.get("margin", 0.0)
            margin_free = account_data.get("margin_free", 0.0)
            currency_str = account_data.get("currency", "USD")

            # Get currency
            currency = Currency.from_str(currency_str)

            # Create balances list with account currency balance
            from nautilus_trader.model.objects import AccountBalance
            from nautilus_trader.model.objects import MarginBalance
            from nautilus_trader.model.objects import Money

            balances = [
                AccountBalance(
                    total=Money(balance, currency),
                    locked=Money(0.0, currency),  # MT5 doesn't provide locked balance
                    free=Money(balance, currency),
                )
            ]

            # Create margins list
            margins = [
                MarginBalance(
                    initial=Money(margin, currency),
                    maintenance=Money(margin, currency),  # MT5 uses same value
                    instrument_id=None,  # Account-level margin
                )
            ]

            # Generate account state
            self.generate_account_state(
                balances=balances,
                margins=margins,
                reported=True,
                ts_event=self._clock.timestamp_ns(),
            )

            self._log.info(
                f"Account state synchronized: Balance={balance} {currency_str}, "
                f"Equity={equity}, Margin={margin}",
                LogColor.GREEN,
            )

        except Exception as e:
            self._log.error(f"Failed to query account state from MT5: {e}")
            # Fall back to minimal account state with USD currency
            from nautilus_trader.model.objects import AccountBalance
            from nautilus_trader.model.objects import Money

            currency = Currency.from_str("USD")
            balances = [
                AccountBalance(
                    total=Money(10000.0, currency),  # Default balance for testing
                    locked=Money(0.0, currency),
                    free=Money(10000.0, currency),
                )
            ]

            self.generate_account_state(
                balances=balances,
                margins=[],
                reported=True,
                ts_event=self._clock.timestamp_ns(),
            )

            self._log.warning(
                f"Using fallback account state with default balance: 10000 USD",
                LogColor.YELLOW,
            )

    # -- EXECUTION REPORTS --------------------------------------------------------------------

    def _get_cache_active_symbols(self) -> set[nautilus_pyo3.Symbol]:
        """
        Get active symbols from cached orders and positions.

        Returns
        -------
        set[nautilus_pyo3.Symbol]
            The set of active symbols.

        """
        open_orders: list[Order] = self._cache.orders_open(venue=self.venue)
        open_positions: list[Position] = self._cache.positions_open(venue=self.venue)
        active_symbols: set[nautilus_pyo3.Symbol] = set()

        for order in open_orders:
            active_symbols.add(nautilus_pyo3.Symbol(order.instrument_id.symbol.value))
        for position in open_positions:
            active_symbols.add(nautilus_pyo3.Symbol(position.instrument_id.symbol.value))

        return active_symbols

    async def generate_order_status_reports(
        self,
        command: GenerateOrderStatusReports,
    ) -> list[OrderStatusReport]:
        """
        Generate order status reports from MT5.

        Parameters
        ----------
        command : GenerateOrderStatusReports
            The command to generate reports.

        Returns
        -------
        list[OrderStatusReport]
            The list of order status reports.

        """
        self._log.debug("Requesting OrderStatusReports from MT5...")
        reports: list[OrderStatusReport] = []

        # Check instruments are cached
        if not self._client.is_initialized():
            await self._cache_instruments()

        # Fetch active symbols from cached state
        active_symbols = self._get_cache_active_symbols()

        try:
            # TODO: Implement request_order_status_reports() in Rust client
            # This should send a query to MT5 via sysSocket and parse responses
            self._log.warning(
                "Order status report generation not yet fully implemented - "
                "querying MT5 for orders",
            )

            # Placeholder: In production, this would call the Rust client
            # pyo3_reports = await self._client.request_order_status_reports(
            #     account_id=self.pyo3_account_id,
            #     symbols=list(active_symbols),
            # )
            #
            # for pyo3_report in pyo3_reports:
            #     report = OrderStatusReport.from_pyo3(pyo3_report)
            #     self._log.debug(f"Received {report}", LogColor.MAGENTA)
            #     reports.append(report)

        except Exception as e:
            self._log.exception("Failed to generate OrderStatusReports", e)

        len_reports = len(reports)
        plural = "" if len_reports == 1 else "s"
        receipt_log = f"Received {len(reports)} OrderStatusReport{plural}"

        if command.log_receipt_level == LogLevel.INFO:
            self._log.info(receipt_log)
        else:
            self._log.debug(receipt_log)

        return reports

    async def generate_order_status_report(
        self,
        command: GenerateOrderStatusReport,
    ) -> OrderStatusReport | None:
        """
        Generate a single order status report from MT5.

        Parameters
        ----------
        command : GenerateOrderStatusReport
            The command to generate a report.

        Returns
        -------
        OrderStatusReport or None
            The order status report, or None if not found.

        """
        PyCondition.is_false(
            command.client_order_id is None and command.venue_order_id is None,
            "both `client_order_id` and `venue_order_id` were `None`",
        )

        # Check instruments are cached
        if not self._client.is_initialized():
            await self._cache_instruments()

        self._log.info(
            f"Requesting OrderStatusReport for "
            f"{repr(command.client_order_id) if command.client_order_id else ''} "
            f"{repr(command.venue_order_id) if command.venue_order_id else ''}",
        )

        if command.venue_order_id is None:
            self._log.warning(
                f"Cannot request order status report for {command.client_order_id}, "
                "order has not been assigned a venue order ID (MT5 ticket)",
            )
            return None

        try:
            # TODO: Implement in Rust client
            # venue_order_id = nautilus_pyo3.VenueOrderId.from_str(command.venue_order_id.value)
            # pyo3_report = await self._client.request_order_status_report(
            #     account_id=self.pyo3_account_id,
            #     venue_order_id=venue_order_id,
            # )
            #
            # report = OrderStatusReport.from_pyo3(pyo3_report)
            # self._log.debug(f"Received {report}", LogColor.MAGENTA)
            # return report
            self._log.warning("Single order status report not yet fully implemented")
            return None
        except Exception as e:
            self._log.exception("Failed to generate OrderStatusReport", e)
        return None

    async def generate_fill_reports(
        self,
        command: GenerateFillReports,
    ) -> list[FillReport]:
        """
        Generate fill reports from MT5.

        Parameters
        ----------
        command : GenerateFillReports
            The command to generate fill reports.

        Returns
        -------
        list[FillReport]
            The list of fill reports.

        """
        # Check instruments cache first
        if not self._client.is_initialized():
            await self._cache_instruments()

        self._log.debug("Requesting FillReports from MT5...")
        reports: list[FillReport] = []

        try:
            # TODO: Implement in Rust client
            # pyo3_reports = await self._client.request_fill_reports(
            #     account_id=self.pyo3_account_id,
            #     start=command.start,
            # )
            #
            # for pyo3_report in pyo3_reports:
            #     report = FillReport.from_pyo3(pyo3_report)
            #     self._log.debug(f"Received {report}", LogColor.MAGENTA)
            #     reports.append(report)
            self._log.warning("Fill report generation not yet fully implemented")
        except Exception as e:
            self._log.exception("Failed to generate FillReports", e)

        len_reports = len(reports)
        plural = "" if len_reports == 1 else "s"
        self._log.info(f"Received {len(reports)} FillReport{plural}")

        return reports

    async def generate_position_status_reports(
        self,
        command: GeneratePositionStatusReports,
    ) -> list[PositionStatusReport]:
        """
        Generate position status reports from MT5.

        Parameters
        ----------
        command : GeneratePositionStatusReports
            The command to generate position reports.

        Returns
        -------
        list[PositionStatusReport]
            The list of position status reports.

        """
        # Check instruments are cached
        if not self._client.is_initialized():
            await self._cache_instruments()

        self._log.debug("Requesting PositionStatusReports from MT5...")
        reports: list[PositionStatusReport] = []

        try:
            # TODO: Implement in Rust client
            # pyo3_reports = await self._client.request_position_status_reports(
            #     account_id=self.pyo3_account_id,
            # )
            #
            # for pyo3_report in pyo3_reports:
            #     report = PositionStatusReport.from_pyo3(pyo3_report)
            #     self._log.debug(f"Received {report}", LogColor.MAGENTA)
            #     reports.append(report)
            self._log.warning("Position report generation not yet fully implemented")
        except Exception as e:
            self._log.exception("Failed to generate PositionReports", e)

        len_reports = len(reports)
        plural = "" if len_reports == 1 else "s"
        self._log.info(f"Received {len(reports)} PositionReport{plural}")

        return reports

    # -- COMMAND HANDLERS ---------------------------------------------------------------------

    async def _query_account(self, _command: QueryAccount) -> None:
        """
        Query account state from MT5.

        """
        await self._update_account_state()

    async def _cancel_order(self, command: CancelOrder) -> None:
        """
        Cancel an order in MT5.

        Parameters
        ----------
        command : CancelOrder
            The cancel order command.

        """
        order: Order | None = self._cache.order(command.client_order_id)
        if order is None:
            self._log.error(f"{command.client_order_id!r} not found in cache")
            return

        if order.is_closed:
            self._log.warning(
                f"`CancelOrder` command for {command.client_order_id!r} when order already {order.status_string()} "
                "(will not send to MT5)",
            )
            return

        try:
            # TODO: Implement in Rust client
            # await self._client.cancel_order(
            #     account_id=self.pyo3_account_id,
            #     venue_order_id=nautilus_pyo3.VenueOrderId(order.venue_order_id.value),
            # )
            self._log.warning(f"Order cancellation not yet fully implemented for {order.venue_order_id}")
        except Exception as e:
            self.generate_order_cancel_rejected(
                order.strategy_id,
                order.instrument_id,
                order.client_order_id,
                order.venue_order_id,
                str(e),
                self._clock.timestamp_ns(),
            )

    async def _cancel_all_orders(self, command: CancelAllOrders) -> None:
        """
        Cancel all orders for an instrument in MT5.

        Parameters
        ----------
        command : CancelAllOrders
            The cancel all orders command.

        """
        instrument = self._cache.instrument(command.instrument_id)
        if instrument is None:
            raise ValueError(f"Instrument {command.instrument_id} not found")

        try:
            # TODO: Implement in Rust client
            # pyo3_order_side: nautilus_pyo3.OrderSide | None = None
            # if command.order_side == OrderSide.BUY:
            #     pyo3_order_side = nautilus_pyo3.OrderSide.BUY
            # elif command.order_side == OrderSide.SELL:
            #     pyo3_order_side = nautilus_pyo3.OrderSide.SELL
            #
            # await self._client.cancel_all_orders(
            #     account_id=self.pyo3_account_id,
            #     symbol=nautilus_pyo3.Symbol(command.instrument_id.symbol.value),
            #     order_side=pyo3_order_side,
            # )
            self._log.warning("Cancel all orders not yet fully implemented")
        except Exception as e:
            orders_open: list[Order] = self._cache.orders_open(instrument_id=command.instrument_id)
            for open_order in orders_open:
                if open_order.is_closed:
                    continue
                self.generate_order_cancel_rejected(
                    open_order.strategy_id,
                    open_order.instrument_id,
                    open_order.client_order_id,
                    open_order.venue_order_id,
                    str(e),
                    self._clock.timestamp_ns(),
                )

    async def _submit_order(self, command: SubmitOrder) -> None:
        """
        Submit an order to MT5.

        Parameters
        ----------
        command : SubmitOrder
            The submit order command.

        """
        order = command.order

        # Validate order type
        if order.order_type not in MT5_SUPPORTED_ORDER_TYPES:
            self._log.error(
                f"MT5 does not support {order.order_type_string()} order types",
            )
            return

        # Validate time in force
        if order.time_in_force not in MT5_SUPPORTED_TIF:
            self._log.error(
                f"MT5 does not support {order.tif_string()} time in force",
            )
            return

        if order.is_closed:
            self._log.warning(f"Cannot submit already closed order, {order}")
            return

        if order.is_quote_quantity:
            reason = "UNSUPPORTED_QUOTE_QUANTITY"
            self._log.error(
                f"Cannot submit order {order.client_order_id}: {reason}",
            )
            self.generate_order_denied(
                strategy_id=order.strategy_id,
                instrument_id=order.instrument_id,
                client_order_id=order.client_order_id,
                reason=reason,
                ts_event=self._clock.timestamp_ns(),
            )
            return

        # Generate order submitted event
        self.generate_order_submitted(
            strategy_id=order.strategy_id,
            instrument_id=order.instrument_id,
            client_order_id=order.client_order_id,
            ts_event=self._clock.timestamp_ns(),
        )

        try:
            # Route to specific order type handler
            if order.order_type == OrderType.MARKET:
                report = await self._submit_market_order(order)
            elif order.order_type == OrderType.LIMIT:
                report = await self._submit_limit_order(order)
            elif order.order_type == OrderType.STOP_MARKET:
                report = await self._submit_stop_market_order(order)
            elif order.order_type == OrderType.STOP_LIMIT:
                report = await self._submit_stop_limit_order(order)
            else:
                self._log.error(f"Submitting {order.type_string()} orders not currently supported")
                return

            # Generate accepted event if we got a report
            if report is not None:
                self.generate_order_accepted(
                    instrument_id=order.instrument_id,
                    strategy_id=order.strategy_id,
                    client_order_id=order.client_order_id,
                    venue_order_id=VenueOrderId(report.venue_order_id.value),
                    ts_event=self._clock.timestamp_ns(),
                )
        except Exception as e:
            self.generate_order_rejected(
                strategy_id=order.strategy_id,
                instrument_id=order.instrument_id,
                client_order_id=order.client_order_id,
                reason=str(e),
                ts_event=self._clock.timestamp_ns(),
            )

    async def _submit_market_order(
        self,
        order: MarketOrder,
    ) -> _Mt5OrderReport | None:
        """
        Submit a market order to MT5.

        Parameters
        ----------
        order : MarketOrder
            The market order to submit.

        Returns
        -------
        _Mt5OrderReport or None
            The order report containing venue_order_id.

        """
        # Convert to PyO3 types
        instrument_id = nautilus_pyo3.InstrumentId.from_str(order.instrument_id.value)
        client_order_id = nautilus_pyo3.ClientOrderId(order.client_order_id.value)
        order_side = order_side_to_pyo3(order.side)
        order_type = nautilus_pyo3.OrderType.MARKET
        quantity = nautilus_pyo3.Quantity.from_str(str(order.quantity))
        price = None  # Market orders don't have a limit price

        # Extract SL/TP if present
        sl = None
        tp = None

        # Comment contains client_order_id for tracking
        comment = order.client_order_id.value

        self._log.info(
            f"Submitting MARKET order to MT5: {order.side} {order.quantity} "
            f"{order.instrument_id.symbol}",
        )

        # Call Rust client
        response = await self._client.submit_order(
            instrument_id,
            client_order_id,
            order_side,
            order_type,
            quantity,
            price,
            sl,
            tp,
            comment,
        )

        # Log full response for debugging
        self._log.info(f"MT5 response received: {response}")

        # Parse response (dict from Rust)
        # Note: MT5 has typo "desription" instead of "description"
        if response.get("error", True):
            error_msg = (
                f"MT5 order submission failed: "
                f"retcode={response.get('retcode', 'unknown')}, "
                f"description={response.get('desription', response.get('description', 'no description'))}"
            )
            self._log.error(error_msg, LogColor.RED)
            raise RuntimeError(error_msg)

        # Extract MT5 ticket number (venue_order_id)
        ticket = response.get("order", 0)
        if ticket == 0:
            raise RuntimeError("MT5 returned ticket 0 - order may not have been placed")

        self._log.info(
            f"Order submitted successfully to MT5: ticket={ticket}, "
            f"fill_price={response.get('price', 'N/A')}",
            LogColor.GREEN,
        )

        # Return report with venue_order_id
        return _Mt5OrderReport(str(ticket))

    async def _submit_limit_order(
        self,
        order: LimitOrder,
    ) -> _Mt5OrderReport | None:
        """
        Submit a limit order to MT5.

        Parameters
        ----------
        order : LimitOrder
            The limit order to submit.

        Returns
        -------
        _Mt5OrderReport or None
            The order report containing venue_order_id.

        """
        # Convert to PyO3 types
        instrument_id = nautilus_pyo3.InstrumentId.from_str(order.instrument_id.value)
        client_order_id = nautilus_pyo3.ClientOrderId(order.client_order_id.value)
        order_side = order_side_to_pyo3(order.side)
        order_type = nautilus_pyo3.OrderType.LIMIT
        quantity = nautilus_pyo3.Quantity.from_str(str(order.quantity))
        price = nautilus_pyo3.Price.from_str(str(order.price))

        # Extract SL/TP if present (from trigger_price for stop orders or exec params)
        sl = None
        tp = None

        # Comment contains client_order_id for tracking
        comment = order.client_order_id.value

        self._log.info(
            f"Submitting LIMIT order to MT5: {order.side} {order.quantity} "
            f"{order.instrument_id.symbol} @ {order.price}",
        )

        # Call Rust client
        response = await self._client.submit_order(
            instrument_id,
            client_order_id,
            order_side,
            order_type,
            quantity,
            price,
            sl,
            tp,
            comment,
        )

        # Log full response for debugging
        self._log.info(f"MT5 response received: {response}")

        # Parse response (dict from Rust)
        # Note: MT5 has typo "desription" instead of "description"
        if response.get("error", True):
            error_msg = (
                f"MT5 order submission failed: "
                f"retcode={response.get('retcode', 'unknown')}, "
                f"description={response.get('desription', response.get('description', 'no description'))}"
            )
            self._log.error(error_msg, LogColor.RED)
            raise RuntimeError(error_msg)

        # Extract MT5 ticket number (venue_order_id)
        ticket = response.get("order", 0)
        if ticket == 0:
            raise RuntimeError("MT5 returned ticket 0 - order may not have been placed")

        self._log.info(
            f"Order submitted successfully to MT5: ticket={ticket}, "
            f"price={response.get('price', 'N/A')}",
            LogColor.GREEN,
        )

        # Return report with venue_order_id
        return _Mt5OrderReport(str(ticket))

    async def _submit_stop_market_order(
        self,
        order: StopMarketOrder,
    ) -> _Mt5OrderReport | None:
        """
        Submit a stop market order to MT5.

        Parameters
        ----------
        order : StopMarketOrder
            The stop market order to submit.

        Returns
        -------
        _Mt5OrderReport or None
            The order report containing venue_order_id.

        """
        # Convert to PyO3 types
        instrument_id = nautilus_pyo3.InstrumentId.from_str(order.instrument_id.value)
        client_order_id = nautilus_pyo3.ClientOrderId(order.client_order_id.value)
        order_side = order_side_to_pyo3(order.side)
        order_type = nautilus_pyo3.OrderType.STOP_MARKET
        quantity = nautilus_pyo3.Quantity.from_str(str(order.quantity))
        price = nautilus_pyo3.Price.from_str(str(order.trigger_price))  # Stop price

        # Extract SL/TP if present
        sl = None
        tp = None

        # Comment contains client_order_id for tracking
        comment = order.client_order_id.value

        self._log.info(
            f"Submitting STOP_MARKET order to MT5: {order.side} {order.quantity} "
            f"{order.instrument_id.symbol} @ stop {order.trigger_price}",
        )

        # Call Rust client
        response = await self._client.submit_order(
            instrument_id,
            client_order_id,
            order_side,
            order_type,
            quantity,
            price,
            sl,
            tp,
            comment,
        )

        # Log full response for debugging
        self._log.info(f"MT5 response received: {response}")

        # Parse response (dict from Rust)
        # Note: MT5 has typo "desription" instead of "description"
        if response.get("error", True):
            error_msg = (
                f"MT5 order submission failed: "
                f"retcode={response.get('retcode', 'unknown')}, "
                f"description={response.get('desription', response.get('description', 'no description'))}"
            )
            self._log.error(error_msg, LogColor.RED)
            raise RuntimeError(error_msg)

        # Extract MT5 ticket number (venue_order_id)
        ticket = response.get("order", 0)
        if ticket == 0:
            raise RuntimeError("MT5 returned ticket 0 - order may not have been placed")

        self._log.info(
            f"Order submitted successfully to MT5: ticket={ticket}",
            LogColor.GREEN,
        )

        # Return report with venue_order_id
        return _Mt5OrderReport(str(ticket))

    async def _submit_stop_limit_order(
        self,
        order: StopLimitOrder,
    ) -> _Mt5OrderReport | None:
        """
        Submit a stop limit order to MT5.

        Parameters
        ----------
        order : StopLimitOrder
            The stop limit order to submit.

        Returns
        -------
        _Mt5OrderReport or None
            The order report containing venue_order_id.

        """
        # Convert to PyO3 types
        instrument_id = nautilus_pyo3.InstrumentId.from_str(order.instrument_id.value)
        client_order_id = nautilus_pyo3.ClientOrderId(order.client_order_id.value)
        order_side = order_side_to_pyo3(order.side)
        order_type = nautilus_pyo3.OrderType.STOP_LIMIT
        quantity = nautilus_pyo3.Quantity.from_str(str(order.quantity))
        # For stop limit: price is the limit price, trigger_price is the stop price
        # MT5 typically uses the trigger_price as the main price and limit as a deviation
        price = nautilus_pyo3.Price.from_str(str(order.trigger_price))

        # Extract SL/TP if present
        sl = None
        tp = None

        # Comment contains client_order_id for tracking
        comment = order.client_order_id.value

        self._log.info(
            f"Submitting STOP_LIMIT order to MT5: {order.side} {order.quantity} "
            f"{order.instrument_id.symbol} @ stop {order.trigger_price} limit {order.price}",
        )

        # Call Rust client
        response = await self._client.submit_order(
            instrument_id,
            client_order_id,
            order_side,
            order_type,
            quantity,
            price,
            sl,
            tp,
            comment,
        )

        # Log full response for debugging
        self._log.info(f"MT5 response received: {response}")

        # Parse response (dict from Rust)
        # Note: MT5 has typo "desription" instead of "description"
        if response.get("error", True):
            error_msg = (
                f"MT5 order submission failed: "
                f"retcode={response.get('retcode', 'unknown')}, "
                f"description={response.get('desription', response.get('description', 'no description'))}"
            )
            self._log.error(error_msg, LogColor.RED)
            raise RuntimeError(error_msg)

        # Extract MT5 ticket number (venue_order_id)
        ticket = response.get("order", 0)
        if ticket == 0:
            raise RuntimeError("MT5 returned ticket 0 - order may not have been placed")

        self._log.info(
            f"Order submitted successfully to MT5: ticket={ticket}",
            LogColor.GREEN,
        )

        # Return report with venue_order_id
        return _Mt5OrderReport(str(ticket))

    def _handle_msg(self, msg: Any) -> None:
        """
        Handle incoming execution messages from MT5.

        This receives order and position updates from the MT5 stream socket.

        Parameters
        ----------
        msg : Any
            The message from the Rust client.

        """
        try:
            # TODO: Handle different message types from MT5 stream
            # - Order status updates (filled, cancelled, rejected)
            # - Position updates
            # - Account balance updates
            if isinstance(msg, nautilus_pyo3.OrderStatusReport):
                report = OrderStatusReport.from_pyo3(msg)
                self._handle_order_report(report)
            elif isinstance(msg, nautilus_pyo3.PositionStatusReport):
                report = PositionStatusReport.from_pyo3(msg)
                self._handle_position_report(report)
            else:
                self._log.warning(f"Unhandled message type: {type(msg)}")
        except Exception as e:
            self._log.exception("Error handling MT5 execution message", e)
