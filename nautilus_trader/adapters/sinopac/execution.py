# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
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
import os
from typing import Any

from nautilus_trader.adapters.sinopac.config import SinopacExecClientConfig
from nautilus_trader.adapters.sinopac.constants import SINOPAC
from nautilus_trader.adapters.sinopac.constants import SINOPAC_VENUE
from nautilus_trader.adapters.sinopac.providers import SinopacInstrumentProvider
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.common.enums import LogColor
from nautilus_trader.core import nautilus_pyo3
from nautilus_trader.core.nautilus_pyo3 import sinopac as pyo3_sinopac
from nautilus_trader.core.nautilus_pyo3.sinopac import SinopacAction
from nautilus_trader.core.nautilus_pyo3.sinopac import SinopacMarket
from nautilus_trader.core.nautilus_pyo3.sinopac import SinopacOrderType
from nautilus_trader.core.nautilus_pyo3.sinopac import SinopacPriceType
from nautilus_trader.core.uuid import UUID4
from nautilus_trader.execution.messages import BatchCancelOrders
from nautilus_trader.execution.messages import CancelAllOrders
from nautilus_trader.execution.messages import CancelOrder
from nautilus_trader.execution.messages import GenerateFillReports
from nautilus_trader.execution.messages import GenerateOrderStatusReport
from nautilus_trader.execution.messages import GenerateOrderStatusReports
from nautilus_trader.execution.messages import GeneratePositionStatusReports
from nautilus_trader.execution.messages import ModifyOrder
from nautilus_trader.execution.messages import SubmitOrder
from nautilus_trader.execution.messages import SubmitOrderList
from nautilus_trader.execution.reports import FillReport
from nautilus_trader.execution.reports import OrderStatusReport
from nautilus_trader.execution.reports import PositionStatusReport
from nautilus_trader.live.cancellation import DEFAULT_FUTURE_CANCELLATION_TIMEOUT
from nautilus_trader.live.cancellation import cancel_tasks_with_timeout
from nautilus_trader.live.execution_client import LiveExecutionClient
from nautilus_trader.model.currencies import Currency
from nautilus_trader.model.enums import AccountType
from nautilus_trader.model.enums import LiquiditySide
from nautilus_trader.model.enums import OmsType
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.enums import OrderStatus
from nautilus_trader.model.enums import OrderType
from nautilus_trader.model.enums import PositionSide
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.identifiers import AccountId
from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import ClientOrderId
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TradeId
from nautilus_trader.model.identifiers import VenueOrderId
from nautilus_trader.model.instruments import Equity
from nautilus_trader.model.instruments import FuturesContract
from nautilus_trader.model.instruments import OptionContract
from nautilus_trader.model.objects import AccountBalance
from nautilus_trader.model.objects import Money


_SINOPAC_STATUS_MAP = {
    "PendingSubmit": OrderStatus.SUBMITTED,
    "PreSubmitted": OrderStatus.SUBMITTED,
    "Submitted": OrderStatus.ACCEPTED,
    "Failed": OrderStatus.REJECTED,
    "Cancelled": OrderStatus.CANCELED,
    "Filled": OrderStatus.FILLED,
    "PartFilled": OrderStatus.PARTIALLY_FILLED,
}

_NT_TO_SINOPAC_ACTION = {
    OrderSide.BUY: SinopacAction.BUY,
    OrderSide.SELL: SinopacAction.SELL,
}

_NT_TO_SINOPAC_PRICE_TYPE = {
    OrderType.LIMIT: SinopacPriceType.LMT,
    OrderType.MARKET: SinopacPriceType.MKT,
}

_NT_TO_SINOPAC_ORDER_TYPE = {
    TimeInForce.DAY: SinopacOrderType.ROD,
    TimeInForce.IOC: SinopacOrderType.IOC,
    TimeInForce.FOK: SinopacOrderType.FOK,
}


class SinopacExecutionClient(LiveExecutionClient):
    """
    Provides an execution client for the Sinopac (SinoPac) adapter.

    Parameters
    ----------
    loop : asyncio.AbstractEventLoop
        The event loop for the client.
    client : pyo3_sinopac.SinopacHttpClient
        The Sinopac gateway HTTP client.
    ws_client : pyo3_sinopac.SinopacWebSocketClient
        The Sinopac gateway WebSocket client.
    msgbus : MessageBus
        The message bus for the client.
    cache : Cache
        The cache for the client.
    clock : LiveClock
        The clock for the client.
    instrument_provider : SinopacInstrumentProvider
        The instrument provider.
    config : SinopacExecClientConfig
        The configuration for the client.
    name : str, optional
        The custom client ID.

    """

    def __init__(
        self,
        loop: asyncio.AbstractEventLoop,
        client: pyo3_sinopac.SinopacHttpClient,
        ws_client: pyo3_sinopac.SinopacWebSocketClient,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
        instrument_provider: SinopacInstrumentProvider,
        config: SinopacExecClientConfig,
        name: str | None = None,
    ) -> None:
        account_id_str = config.account_id or os.environ.get(
            "SINOPAC_ACCOUNT_ID",
            "SINOPAC-001",
        )
        account_id = AccountId(f"{SINOPAC}-{account_id_str}")

        super().__init__(
            loop=loop,
            client_id=ClientId(name or SINOPAC),
            venue=SINOPAC_VENUE,
            oms_type=OmsType.NETTING,
            account_type=AccountType.CASH,
            base_currency=None,
            instrument_provider=instrument_provider,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            config=config,
        )

        self._http_client = client
        self._ws_client = ws_client
        self._config = config
        self._set_account_id(account_id)
        self._client_futures: set[asyncio.Future] = set()

        # Maps trade_id (VenueOrderId) → client_order_id for WS event correlation
        self._trade_id_to_client_order_id: dict[str, str] = {}

    # -- Connection lifecycle -------------------------------------------------

    async def _connect(self) -> None:
        await self._instrument_provider.initialize()
        await self._update_account_state()
        self._log.info(
            f"Connected to Sinopac gateway at {self._config.gateway_base_url}",
            LogColor.GREEN,
        )

    async def _disconnect(self) -> None:
        await cancel_tasks_with_timeout(
            self._client_futures,
            self._log,
            timeout_secs=DEFAULT_FUTURE_CANCELLATION_TIMEOUT,
        )
        self._client_futures.clear()

    async def _update_account_state(self) -> None:
        try:
            balance_data = await self._http_client.account_balance()
            twd = Currency.from_str("TWD")
            balances = [
                AccountBalance(
                    total=Money(balance_data["balance"], twd),
                    locked=Money(0, twd),
                    free=Money(balance_data["balance"], twd),
                ),
            ]
            self.generate_account_state(
                balances=balances,
                margins=[],
                reported=True,
                ts_event=self._clock.timestamp_ns(),
            )
        except Exception as e:
            self._log.error(f"Failed to update account state: {e}")

    # -- WS message handler ---------------------------------------------------

    def _handle_msg(self, msg: object) -> None:
        try:
            if nautilus_pyo3.is_pycapsule(msg):
                return  # Market data -- handled by DataClient

            if isinstance(msg, dict):
                self._handle_order_event(msg)
                return

            self._log.warning(f"Unhandled exec WS message type: {type(msg)}")
        except Exception as e:
            self._log.exception("Error handling Sinopac exec WS message", e)

    def _handle_order_event(self, event: dict[str, Any]) -> None:
        event_type = event.get("event_type")
        if event_type in ("stock_order", "futures_order"):
            self._handle_order_status_event(event)
        elif event_type in ("stock_deal", "futures_deal"):
            self._handle_deal_event(event)
        else:
            self._log.warning(f"Unknown order event type: {event_type}")

    def _handle_order_status_event(self, event: dict[str, Any]) -> None:
        op_code = event.get("op_code", "")
        op_type = event.get("op_type", "")
        order_id = event.get("order_id", "")
        code = event.get("code", "")

        instrument_id = InstrumentId.from_str(f"{code}.{SINOPAC}")

        # Look up the NT order via trade_id → client_order_id mapping
        client_order_id_str = self._trade_id_to_client_order_id.get(order_id)
        if client_order_id_str is None:
            self._log.info(f"External order event: {op_type} {order_id} {code}")
            return

        client_order_id = ClientOrderId(client_order_id_str)
        order = self._cache.order(client_order_id)
        if order is None:
            self._log.warning(f"Order {client_order_id} not found in cache for event")
            return

        venue_order_id = VenueOrderId(order_id)
        ts_event = self._clock.timestamp_ns()

        if op_code != "00":
            # Operation failed
            reason = event.get("op_msg", f"Operation failed: {op_type} code={op_code}")
            if op_type == "New":
                # A "New" failure that arrives AFTER the order was already accepted
                # (HTTP place_order succeeded) would drive an illegal
                # ACCEPTED/PARTIALLY_FILLED/FILLED -> REJECTED transition and panic
                # NT's Rust state machine. Only reject orders that are still pending.
                if order.status in (
                    OrderStatus.ACCEPTED,
                    OrderStatus.PARTIALLY_FILLED,
                    OrderStatus.FILLED,
                ):
                    self._log.warning(
                        f"Late 'New' failure for {client_order_id} in {order.status!r} "
                        f"(reason={reason}); ignoring to avoid illegal state transition",
                    )
                    return
                self.generate_order_rejected(
                    strategy_id=order.strategy_id,
                    instrument_id=instrument_id,
                    client_order_id=client_order_id,
                    reason=reason,
                    ts_event=ts_event,
                )
                self._trade_id_to_client_order_id.pop(order_id, None)
            elif op_type == "Cancel":
                self.generate_order_cancel_rejected(
                    strategy_id=order.strategy_id,
                    instrument_id=instrument_id,
                    client_order_id=client_order_id,
                    venue_order_id=venue_order_id,
                    reason=reason,
                    ts_event=ts_event,
                )
            elif op_type in ("UpdatePrice", "UpdateQty"):
                self.generate_order_modify_rejected(
                    strategy_id=order.strategy_id,
                    instrument_id=instrument_id,
                    client_order_id=client_order_id,
                    venue_order_id=venue_order_id,
                    reason=reason,
                    ts_event=ts_event,
                )
            return

        # Operation succeeded (op_code == "00")
        if op_type == "Cancel":
            self.generate_order_canceled(
                strategy_id=order.strategy_id,
                instrument_id=instrument_id,
                client_order_id=client_order_id,
                venue_order_id=venue_order_id,
                ts_event=ts_event,
            )
            self._trade_id_to_client_order_id.pop(order_id, None)
        elif op_type in ("UpdatePrice", "UpdateQty"):
            modified_price = event.get("modified_price", 0.0)
            order_quantity = event.get("order_quantity", 0)
            instrument = self._cache.instrument(instrument_id)
            if instrument is not None:
                self.generate_order_updated(
                    strategy_id=order.strategy_id,
                    instrument_id=instrument_id,
                    client_order_id=client_order_id,
                    venue_order_id=venue_order_id,
                    quantity=instrument.make_qty(order_quantity),
                    price=instrument.make_price(modified_price)
                    if modified_price > 0
                    else order.price,
                    trigger_price=None,
                    ts_event=ts_event,
                )
        # "New" with op_code "00" = order accepted (already handled in _submit_order)

    def _handle_deal_event(self, event: dict[str, Any]) -> None:
        trade_id_str = event.get("trade_id", "")
        ordno = event.get("ordno", "")
        # `seqno` is unique per fill; `ordno` (brokerage order number) is shared by
        # all partial fills of one order. Prefer `seqno` so partial-fill TradeIds do
        # not collide and corrupt the ledger; fall back to `ordno` if `seqno` absent.
        seq = event.get("seqno") or ordno
        code = event.get("code", "")
        price = event.get("price", 0.0)
        quantity = event.get("quantity", 0)
        ts = event.get("ts", 0.0)

        instrument_id = InstrumentId.from_str(f"{code}.{SINOPAC}")
        instrument = self._cache.instrument(instrument_id)
        if instrument is None:
            self._log.error(f"Cannot process deal: instrument {instrument_id} not in cache")
            return

        # Look up the NT order
        client_order_id_str = self._trade_id_to_client_order_id.get(trade_id_str)
        if client_order_id_str is None:
            self._log.info(
                f"External deal: {code} {event.get('action', '')} {price}x{quantity}",
            )
            return

        client_order_id = ClientOrderId(client_order_id_str)
        order = self._cache.order(client_order_id)
        if order is None:
            self._log.warning(f"Order {client_order_id} not found for deal event")
            return

        venue_order_id = order.venue_order_id or VenueOrderId(trade_id_str)
        ts_event_ns = int(ts * 1_000_000_000) if ts > 0 else self._clock.timestamp_ns()

        twd = Currency.from_str("TWD")
        self.generate_order_filled(
            strategy_id=order.strategy_id,
            instrument_id=instrument_id,
            client_order_id=client_order_id,
            venue_order_id=venue_order_id,
            venue_position_id=None,
            trade_id=TradeId(f"{trade_id_str}-{seq}"),
            order_side=order.side,
            order_type=order.order_type,
            last_qty=instrument.make_qty(quantity),
            last_px=instrument.make_price(price),
            quote_currency=twd,
            commission=Money(0, twd),
            liquidity_side=LiquiditySide.NO_LIQUIDITY_SIDE,
            ts_event=ts_event_ns,
        )

        # Clean up mapping when order is fully filled
        order = self._cache.order(client_order_id)
        if order is not None and order.is_closed:
            self._trade_id_to_client_order_id.pop(trade_id_str, None)

    # -- Order operations -----------------------------------------------------

    async def _submit_order(self, command: SubmitOrder) -> None:
        order = command.order
        instrument_id = order.instrument_id

        if order.order_type not in _NT_TO_SINOPAC_PRICE_TYPE:
            self._log.error(f"Unsupported order type: {order.order_type}")
            return

        self.generate_order_submitted(
            strategy_id=order.strategy_id,
            instrument_id=instrument_id,
            client_order_id=order.client_order_id,
            ts_event=self._clock.timestamp_ns(),
        )

        try:
            code = instrument_id.symbol.value
            action = _NT_TO_SINOPAC_ACTION[order.side]
            price_type = _NT_TO_SINOPAC_PRICE_TYPE[order.order_type]
            order_type = _NT_TO_SINOPAC_ORDER_TYPE.get(
                order.time_in_force,
                SinopacOrderType.ROD,
            )
            price = float(order.price) if order.price is not None else 0.0
            quantity = int(order.quantity)

            instrument = self._cache.instrument(instrument_id)
            market = self._determine_market(instrument)

            response = await self._http_client.place_order(
                code=code,
                action=action,
                price=price,
                quantity=quantity,
                price_type=price_type,
                order_type=order_type,
                market=market,
            )

            trade_id = response["trade_id"]
            venue_order_id = VenueOrderId(trade_id)

            self._trade_id_to_client_order_id[trade_id] = order.client_order_id.value

            self.generate_order_accepted(
                strategy_id=order.strategy_id,
                instrument_id=instrument_id,
                client_order_id=order.client_order_id,
                venue_order_id=venue_order_id,
                ts_event=self._clock.timestamp_ns(),
            )

        except (asyncio.TimeoutError, OSError) as e:
            # Transport failure: the request may have actually reached the gateway
            # and the order may be LIVE on the exchange. Rejecting here would create
            # hidden exposure (we report REJECTED while the venue holds a working
            # order). Leave the order in SUBMITTED and let WS deal/order events or
            # reconciliation (`generate_order_status_reports` -> `list_trades`)
            # adopt the true state. Reconciliation keys on the venue `trade_id`, so
            # it can re-establish the order even though the in-memory
            # `_trade_id_to_client_order_id` mapping was never set (we never received
            # a `trade_id` on timeout).
            self._log.error(
                f"place_order transport failure for {order.client_order_id}: {e!r}; "
                f"leaving order SUBMITTED for WS/reconciliation to resolve",
            )
        except Exception as e:
            # Genuine business rejection (validation, margin, unsupported params, ...):
            # the order definitively did not reach a working state, so reject is safe.
            self.generate_order_rejected(
                strategy_id=order.strategy_id,
                instrument_id=instrument_id,
                client_order_id=order.client_order_id,
                reason=str(e),
                ts_event=self._clock.timestamp_ns(),
            )

    async def _modify_order(self, command: ModifyOrder) -> None:
        order = self._cache.order(command.client_order_id)
        if order is None:
            self._log.error(
                f"Cannot modify: order {command.client_order_id} not found in cache",
            )
            return
        if order.is_closed:
            self._log.warning(
                f"Cannot modify: order {command.client_order_id} already closed",
            )
            return

        venue_order_id = order.venue_order_id
        if venue_order_id is None:
            self._log.error(
                f"Cannot modify: no venue_order_id for {command.client_order_id}",
            )
            return

        try:
            trade_id = venue_order_id.value
            price = float(command.price) if command.price is not None else None
            quantity = int(command.quantity) if command.quantity is not None else None

            await self._http_client.update_order(
                trade_id=trade_id,
                price=price,
                quantity=quantity,
            )
            # Actual update confirmation comes via WS order event callback

        except Exception as e:
            self.generate_order_modify_rejected(
                strategy_id=order.strategy_id,
                instrument_id=order.instrument_id,
                client_order_id=order.client_order_id,
                venue_order_id=venue_order_id,
                reason=str(e),
                ts_event=self._clock.timestamp_ns(),
            )

    async def _cancel_order(self, command: CancelOrder) -> None:
        order = self._cache.order(command.client_order_id)
        if order is None:
            self._log.error(
                f"Cannot cancel: order {command.client_order_id} not found in cache",
            )
            return
        if order.is_closed:
            self._log.warning(
                f"Cannot cancel: order {command.client_order_id} already closed",
            )
            return

        venue_order_id = order.venue_order_id
        if venue_order_id is None:
            self._log.error(
                f"Cannot cancel: no venue_order_id for {command.client_order_id}",
            )
            return

        try:
            await self._http_client.cancel_order(trade_id=venue_order_id.value)
            # Actual cancel confirmation comes via WS order event callback

        except Exception as e:
            self.generate_order_cancel_rejected(
                strategy_id=order.strategy_id,
                instrument_id=order.instrument_id,
                client_order_id=order.client_order_id,
                venue_order_id=venue_order_id,
                reason=str(e),
                ts_event=self._clock.timestamp_ns(),
            )

    async def _cancel_all_orders(self, command: CancelAllOrders) -> None:
        open_orders = self._cache.orders_open(instrument_id=command.instrument_id)
        for order in open_orders:
            if order.venue_order_id is not None:
                try:
                    await self._http_client.cancel_order(
                        trade_id=order.venue_order_id.value,
                    )
                except Exception as e:
                    self._log.error(f"Failed to cancel {order.client_order_id}: {e}")

    async def _submit_order_list(self, command: SubmitOrderList) -> None:
        for order in command.order_list.orders:
            submit = SubmitOrder(
                trader_id=command.trader_id,
                strategy_id=command.strategy_id,
                order=order,
                command_id=command.id,
                ts_init=command.ts_init,
            )
            await self._submit_order(submit)

    async def _batch_cancel_orders(self, command: BatchCancelOrders) -> None:
        for cancel in command.cancels:
            await self._cancel_order(cancel)

    def _determine_market(self, instrument: object) -> SinopacMarket:
        if isinstance(instrument, Equity):
            return SinopacMarket.STOCK
        elif isinstance(instrument, FuturesContract):
            return SinopacMarket.FUTURES
        elif isinstance(instrument, OptionContract):
            return SinopacMarket.OPTIONS
        return SinopacMarket.STOCK

    # -- Reconciliation reports -----------------------------------------------

    async def generate_order_status_reports(
        self,
        command: GenerateOrderStatusReports,
    ) -> list[OrderStatusReport]:
        reports: list[OrderStatusReport] = []
        try:
            trades = await self._http_client.list_trades()
            for trade_dict in trades:
                instrument_id = InstrumentId.from_str(
                    f"{trade_dict['code']}.{SINOPAC}",
                )

                if command.instrument_id and command.instrument_id != instrument_id:
                    continue

                instrument = self._cache.instrument(instrument_id)
                if instrument is None:
                    continue

                raw_status = trade_dict["status"]
                # Gateway may return "Status.Failed" instead of "Failed"
                status_key = raw_status.split(".")[-1] if "." in raw_status else raw_status
                order_status = _SINOPAC_STATUS_MAP.get(status_key)
                if order_status is None:
                    self._log.warning(
                        f"Unknown Sinopac order status '{raw_status}', defaulting to DENIED",
                    )
                    order_status = OrderStatus.DENIED
                order_side = OrderSide.BUY if trade_dict["action"] == "Buy" else OrderSide.SELL
                order_type = (
                    OrderType.LIMIT if trade_dict["price_type"] == "LMT" else OrderType.MARKET
                )

                # Map gateway order_type to NT TimeInForce
                raw_order_type = trade_dict.get("order_type", "ROD")
                tif_key = raw_order_type.split(".")[-1] if "." in raw_order_type else raw_order_type
                tif_map = {"ROD": TimeInForce.DAY, "IOC": TimeInForce.IOC, "FOK": TimeInForce.FOK}
                time_in_force = tif_map.get(tif_key, TimeInForce.DAY)

                trade_id = trade_dict["trade_id"]
                client_order_id_str = self._trade_id_to_client_order_id.get(trade_id)
                client_order_id = (
                    ClientOrderId(client_order_id_str)
                    if client_order_id_str
                    else ClientOrderId(f"SINOPAC-{trade_id}")
                )

                filled_qty = trade_dict.get("filled_qty", 0)

                report = OrderStatusReport(
                    account_id=self.account_id,
                    instrument_id=instrument_id,
                    client_order_id=client_order_id,
                    venue_order_id=VenueOrderId(trade_id),
                    order_side=order_side,
                    order_type=order_type,
                    time_in_force=time_in_force,
                    quantity=instrument.make_qty(trade_dict["quantity"]),
                    filled_qty=instrument.make_qty(filled_qty),
                    price=instrument.make_price(trade_dict["price"]),
                    order_status=order_status,
                    report_id=UUID4(),
                    ts_accepted=self._clock.timestamp_ns(),
                    ts_last=self._clock.timestamp_ns(),
                    ts_init=self._clock.timestamp_ns(),
                )
                reports.append(report)
        except Exception as e:
            self._log.error(f"Failed to generate order status reports: {e}")

        return reports

    async def generate_order_status_report(
        self,
        command: GenerateOrderStatusReport,
    ) -> OrderStatusReport | None:
        reports = await self.generate_order_status_reports(
            GenerateOrderStatusReports(
                trader_id=command.trader_id,
                instrument_id=command.instrument_id,
                command_id=command.id,
                ts_init=command.ts_init,
            ),
        )
        for report in reports:
            if command.client_order_id and report.client_order_id == command.client_order_id:
                return report
            if command.venue_order_id and report.venue_order_id == command.venue_order_id:
                return report
        return None

    async def generate_fill_reports(
        self,
        command: GenerateFillReports,
    ) -> list[FillReport]:
        self._log.info(
            "Fill reports generated from WS events only (no historical fill endpoint)",
        )
        return []

    async def generate_position_status_reports(
        self,
        command: GeneratePositionStatusReports,
    ) -> list[PositionStatusReport]:
        reports: list[PositionStatusReport] = []
        try:
            for market in ("stock", "futures"):
                try:
                    positions = await self._http_client.list_positions(market=market)
                except Exception as e:
                    self._log.debug(f"No {market} positions available: {e}")
                    continue

                for pos_dict in positions:
                    instrument_id = InstrumentId.from_str(
                        f"{pos_dict['code']}.{SINOPAC}",
                    )

                    if command.instrument_id and command.instrument_id != instrument_id:
                        continue

                    instrument = self._cache.instrument(instrument_id)
                    if instrument is None:
                        continue

                    direction = pos_dict.get("direction", "")
                    if direction == "Buy":
                        position_side = PositionSide.LONG
                    elif direction == "Sell":
                        position_side = PositionSide.SHORT
                    else:
                        position_side = PositionSide.FLAT

                    quantity = pos_dict.get("quantity", 0)
                    if quantity == 0:
                        continue

                    report = PositionStatusReport(
                        account_id=self.account_id,
                        instrument_id=instrument_id,
                        position_side=position_side,
                        quantity=instrument.make_qty(quantity),
                        report_id=UUID4(),
                        ts_last=self._clock.timestamp_ns(),
                        ts_init=self._clock.timestamp_ns(),
                    )
                    reports.append(report)
        except Exception as e:
            self._log.error(f"Failed to generate position status reports: {e}")

        return reports
