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

import asyncio
from unittest.mock import AsyncMock
from unittest.mock import MagicMock

import pytest

from nautilus_trader.adapters.sinopac.config import SinopacExecClientConfig
from nautilus_trader.adapters.sinopac.execution import SinopacExecutionClient
from nautilus_trader.adapters.sinopac.providers import SinopacInstrumentProvider
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.core.nautilus_pyo3 import sinopac as pyo3_sinopac
from nautilus_trader.execution.messages import SubmitOrder
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.enums import OrderStatus
from nautilus_trader.model.identifiers import TradeId
from nautilus_trader.test_kit.providers import TestInstrumentProvider
from nautilus_trader.test_kit.stubs.component import TestComponentStubs
from nautilus_trader.test_kit.stubs.execution import TestExecStubs
from nautilus_trader.test_kit.stubs.identifiers import TestIdStubs


# -- Harness ----------------------------------------------------------------------------------------


@pytest.fixture
def sinopac_equity():
    return TestInstrumentProvider.equity(symbol="2330", venue="SINOPAC")


@pytest.fixture
def exec_client(event_loop, sinopac_equity):
    """
    Build a SinopacExecutionClient with mocked pyo3 transports and a real cache.

    The pyo3 HTTP/WS clients are mocked because they require a live gateway. The
    NT MessageBus / Cache / clock are real so order-state checks are genuine.

    """
    clock = LiveClock()
    trader_id = TestIdStubs.trader_id()
    msgbus = MessageBus(trader_id, clock)
    cache = TestComponentStubs.cache()
    cache.add_instrument(sinopac_equity)

    http_client = MagicMock(spec=pyo3_sinopac.SinopacHttpClient)
    http_client.place_order = AsyncMock()
    ws_client = MagicMock(spec=pyo3_sinopac.SinopacWebSocketClient)
    provider = MagicMock(spec=SinopacInstrumentProvider)

    client = SinopacExecutionClient(
        loop=event_loop,
        client=http_client,
        ws_client=ws_client,
        msgbus=msgbus,
        cache=cache,
        clock=clock,
        instrument_provider=provider,
        config=SinopacExecClientConfig(),
        name=None,
    )
    return client


def _add_accepted_order(client, instrument, *, client_order_id=None, venue_order_id=None):
    """
    Create an ACCEPTED order, register it in the cache, and seed the WS mapping.
    """
    from nautilus_trader.model.identifiers import VenueOrderId

    venue_order_id = venue_order_id or VenueOrderId("T0001")
    order = TestExecStubs.make_accepted_order(
        instrument=instrument,
        order_side=OrderSide.BUY,
        quantity=instrument.make_qty(2000),
        price=instrument.make_price(580.0),
        client_order_id=client_order_id,
        venue_order_id=venue_order_id,
    )
    client._cache.add_order(order)
    client._trade_id_to_client_order_id[venue_order_id.value] = order.client_order_id.value
    return order, venue_order_id


# -- P1: unique fill TradeId via seqno --------------------------------------------------------------


def test_p1_partial_fills_same_ordno_distinct_seqno_yield_distinct_trade_ids(
    exec_client,
    sinopac_equity,
):
    """
    Two partial fills of one order share `ordno` but have distinct `seqno`.

    The resulting NT TradeIds MUST be distinct, otherwise NT fill dedup drops the
    second fill and the ledger is corrupted (P1).

    """
    # Arrange
    order, venue_order_id = _add_accepted_order(exec_client, sinopac_equity)
    captured_trade_ids: list[TradeId] = []

    def _capture(*args, **kwargs):
        captured_trade_ids.append(kwargs["trade_id"])

    exec_client.generate_order_filled = MagicMock(side_effect=_capture)

    base_event = {
        "event_type": "stock_deal",
        "trade_id": venue_order_id.value,
        "ordno": "A1234",  # identical across both partial fills
        "code": "2330",
        "action": "Buy",
        "ts": 1709352601.0,
    }
    fill_1 = {**base_event, "seqno": "000001", "price": 580.0, "quantity": 1000}
    fill_2 = {**base_event, "seqno": "000002", "price": 580.0, "quantity": 1000}

    # Act
    exec_client._handle_deal_event(fill_1)
    exec_client._handle_deal_event(fill_2)

    # Assert
    assert len(captured_trade_ids) == 2
    assert captured_trade_ids[0] != captured_trade_ids[1], "duplicate TradeId corrupts ledger"
    assert captured_trade_ids[0] == TradeId(f"{venue_order_id.value}-000001")
    assert captured_trade_ids[1] == TradeId(f"{venue_order_id.value}-000002")


def test_p1_falls_back_to_ordno_when_seqno_absent(exec_client, sinopac_equity):
    """
    If `seqno` is missing (e.g. pre-rebuild gateway), fall back to `ordno`.
    """
    # Arrange
    order, venue_order_id = _add_accepted_order(exec_client, sinopac_equity)
    captured: list[TradeId] = []
    exec_client.generate_order_filled = MagicMock(
        side_effect=lambda *a, **k: captured.append(k["trade_id"]),
    )

    event = {
        "event_type": "stock_deal",
        "trade_id": venue_order_id.value,
        "ordno": "A1234",
        "code": "2330",
        "action": "Buy",
        "price": 580.0,
        "quantity": 1000,
        "ts": 1709352601.0,
        # no seqno key
    }

    # Act
    exec_client._handle_deal_event(event)

    # Assert
    assert captured == [TradeId(f"{venue_order_id.value}-A1234")]


# -- P2: late "New" failure must not illegally reject an accepted order -----------------------------


def test_p2_late_new_failure_on_accepted_order_is_ignored(exec_client, sinopac_equity):
    """
    A WS `op_type="New", op_code!="00"` arriving after the order is ACCEPTED must
    not drive an illegal ACCEPTED -> REJECTED transition (state-machine panic, P2).

    """
    # Arrange
    order, venue_order_id = _add_accepted_order(exec_client, sinopac_equity)
    assert order.status == OrderStatus.ACCEPTED

    exec_client.generate_order_rejected = MagicMock()

    late_failure = {
        "event_type": "stock_order",
        "op_type": "New",
        "op_code": "99",  # secondary exchange rejection
        "op_msg": "exchange rejected after accept",
        "order_id": venue_order_id.value,
        "code": "2330",
    }

    # Act (must not raise)
    exec_client._handle_order_status_event(late_failure)

    # Assert
    exec_client.generate_order_rejected.assert_not_called()
    assert order.status == OrderStatus.ACCEPTED
    # Mapping retained so subsequent fills can still be correlated
    assert venue_order_id.value in exec_client._trade_id_to_client_order_id


def test_p2_new_failure_before_accept_still_rejects(exec_client, sinopac_equity):
    """
    A genuine "New" failure on a still-pending (SUBMITTED) order must reject.
    """
    # Arrange
    from nautilus_trader.model.identifiers import VenueOrderId

    venue_order_id = VenueOrderId("T0002")
    order = TestExecStubs.make_submitted_order(
        instrument=sinopac_equity,
        order_side=OrderSide.BUY,
        quantity=sinopac_equity.make_qty(2000),
        price=sinopac_equity.make_price(580.0),
    )
    exec_client._cache.add_order(order)
    exec_client._trade_id_to_client_order_id[venue_order_id.value] = order.client_order_id.value
    assert order.status == OrderStatus.SUBMITTED

    exec_client.generate_order_rejected = MagicMock()

    failure = {
        "event_type": "stock_order",
        "op_type": "New",
        "op_code": "99",
        "op_msg": "rejected at submission",
        "order_id": venue_order_id.value,
        "code": "2330",
    }

    # Act
    exec_client._handle_order_status_event(failure)

    # Assert
    exec_client.generate_order_rejected.assert_called_once()


# -- P3: transport timeout must not reject (keep pending for WS/reconciliation) ---------------------


@pytest.mark.asyncio
async def test_p3_timeout_does_not_reject_order(exec_client, sinopac_equity):
    """
    On HTTP transport timeout the order may be live on the exchange. The client
    MUST NOT reject it (which would create hidden exposure); it stays SUBMITTED
    for WS events / reconciliation to resolve (P3).

    """
    # Arrange
    order = TestExecStubs.limit_order(
        instrument=sinopac_equity,
        order_side=OrderSide.BUY,
        quantity=sinopac_equity.make_qty(2000),
        price=sinopac_equity.make_price(580.0),
    )
    exec_client._cache.add_order(order)

    exec_client.generate_order_submitted = MagicMock()
    exec_client.generate_order_accepted = MagicMock()
    exec_client.generate_order_rejected = MagicMock()
    exec_client._http_client.place_order = AsyncMock(side_effect=asyncio.TimeoutError())

    command = SubmitOrder(
        trader_id=order.trader_id,
        strategy_id=order.strategy_id,
        order=order,
        command_id=TestIdStubs.uuid(),
        ts_init=0,
    )

    # Act
    await exec_client._submit_order(command)

    # Assert
    exec_client.generate_order_submitted.assert_called_once()
    exec_client.generate_order_rejected.assert_not_called()
    exec_client.generate_order_accepted.assert_not_called()


@pytest.mark.asyncio
async def test_p3_business_rejection_still_rejects(exec_client, sinopac_equity):
    """
    A genuine business rejection (non-transport exception) MUST still reject.
    """
    # Arrange
    order = TestExecStubs.limit_order(
        instrument=sinopac_equity,
        order_side=OrderSide.BUY,
        quantity=sinopac_equity.make_qty(2000),
        price=sinopac_equity.make_price(580.0),
    )
    exec_client._cache.add_order(order)

    exec_client.generate_order_submitted = MagicMock()
    exec_client.generate_order_rejected = MagicMock()
    exec_client._http_client.place_order = AsyncMock(
        side_effect=ValueError("insufficient margin"),
    )

    command = SubmitOrder(
        trader_id=order.trader_id,
        strategy_id=order.strategy_id,
        order=order,
        command_id=TestIdStubs.uuid(),
        ts_init=0,
    )

    # Act
    await exec_client._submit_order(command)

    # Assert
    exec_client.generate_order_rejected.assert_called_once()
