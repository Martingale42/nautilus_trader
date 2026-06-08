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


# -- P1: per-fill-unique TradeId via exchange_seq / deal-level ordno --------------------------------
#
# Per the official Shioaji deal-event semantics
# (sinotrade.github.io/tutor/order_deal_event):
#   - `seqno`        == the ORDER's seqno  -> SAME across all partial fills of an order.
#   - `ordno`        == deal-level order number, last 3 chars = deal sequence -> per-fill UNIQUE.
#   - `exchange_seq` == exchange per-deal sequence -> per-fill UNIQUE (may be absent).
# The fill TradeId key is `exchange_seq or ordno`; keying on `seqno` would collide.


def test_p1_partial_fills_same_seqno_distinct_exchange_seq_yield_distinct_trade_ids(
    exec_client,
    sinopac_equity,
):
    """
    Two partial fills of one order share `trade_id` AND `seqno` (per-ORDER) but
    carry distinct per-fill `exchange_seq` (and distinct deal-level `ordno`).

    The resulting NT TradeIds MUST be distinct, otherwise NT fill dedup drops the
    second fill and the ledger is corrupted (P1). This is the real Shioaji shape:
    `seqno` repeats across fills, so it can NEVER be the fill key.

    """
    # Arrange
    order, venue_order_id = _add_accepted_order(exec_client, sinopac_equity)
    captured_trade_ids: list[TradeId] = []
    captured_qtys: list[int] = []

    def _capture(*args, **kwargs):
        captured_trade_ids.append(kwargs["trade_id"])
        captured_qtys.append(int(kwargs["last_qty"]))

    exec_client.generate_order_filled = MagicMock(side_effect=_capture)

    # SAME seqno across both fills (per-ORDER), distinct exchange_seq + deal-level ordno.
    base_event = {
        "event_type": "stock_deal",
        "trade_id": venue_order_id.value,
        "seqno": "123456",  # per-ORDER: identical across both partial fills
        "code": "2330",
        "action": "Buy",
        "ts": 1709352601.0,
    }
    fill_1 = {
        **base_event,
        "ordno": "tA0deX001",
        "exchange_seq": "E0001",
        "price": 580.0,
        "quantity": 1000,
    }
    fill_2 = {
        **base_event,
        "ordno": "tA0deX002",
        "exchange_seq": "E0002",
        "price": 580.0,
        "quantity": 1000,
    }

    # Act
    exec_client._handle_deal_event(fill_1)
    exec_client._handle_deal_event(fill_2)

    # Assert -- distinct TradeIds keyed on the per-fill-unique exchange_seq.
    assert len(captured_trade_ids) == 2
    assert captured_trade_ids[0] != captured_trade_ids[1], "duplicate TradeId corrupts ledger"
    assert captured_trade_ids[0] == TradeId(f"{venue_order_id.value}-E0001")
    assert captured_trade_ids[1] == TradeId(f"{venue_order_id.value}-E0002")

    # Regression guard: keying on the (identical) seqno WOULD collide. Prove the new
    # key does not, even though seqno is byte-for-byte identical across both fills.
    assert fill_1["seqno"] == fill_2["seqno"]
    seqno_key_1 = TradeId(f"{venue_order_id.value}-{fill_1['seqno']}")
    seqno_key_2 = TradeId(f"{venue_order_id.value}-{fill_2['seqno']}")
    assert seqno_key_1 == seqno_key_2, "sanity: identical seqno collides under a seqno key"
    assert captured_trade_ids[0] != seqno_key_1, "new key must NOT reduce to the seqno key"
    assert captured_trade_ids[1] != seqno_key_2, "new key must NOT reduce to the seqno key"

    # Both fills must be counted (no dedup-drop): the position aggregates 1000 + 1000.
    assert captured_qtys == [1000, 1000]
    assert sum(captured_qtys) == 2000


def test_p1_falls_back_to_ordno_when_exchange_seq_absent(exec_client, sinopac_equity):
    """
    When `exchange_seq` is absent (e.g. simulation / pre-confirmation), the key
    falls back to the per-fill-unique deal-level `ordno` and fills stay distinct.

    """
    # Arrange
    order, venue_order_id = _add_accepted_order(exec_client, sinopac_equity)
    captured: list[TradeId] = []
    exec_client.generate_order_filled = MagicMock(
        side_effect=lambda *a, **k: captured.append(k["trade_id"]),
    )

    base_event = {
        "event_type": "stock_deal",
        "trade_id": venue_order_id.value,
        "seqno": "123456",  # per-ORDER: identical across both fills
        "code": "2330",
        "action": "Buy",
        "price": 580.0,
        "quantity": 1000,
        "ts": 1709352601.0,
        # no exchange_seq key -> fall back to deal-level ordno
    }
    fill_1 = {**base_event, "ordno": "tA0deX001"}
    fill_2 = {**base_event, "ordno": "tA0deX002"}

    # Act
    exec_client._handle_deal_event(fill_1)
    exec_client._handle_deal_event(fill_2)

    # Assert -- fallback ordno keeps fills distinct even with identical seqno.
    assert captured == [
        TradeId(f"{venue_order_id.value}-tA0deX001"),
        TradeId(f"{venue_order_id.value}-tA0deX002"),
    ]
    assert captured[0] != captured[1], "fallback ordno must stay per-fill unique"


def test_p1_seqno_key_would_collide_proves_regression(exec_client, sinopac_equity):
    """
    Regression guard for the P1 fix: keying on `seqno` (per-ORDER) collides.

    Builds two real-shaped partial fills with IDENTICAL `seqno` and asserts the
    emitted TradeIds are distinct -- i.e. the implementation does NOT key on seqno.
    If someone reverts the key back to `seqno`, both fills collapse to the same
    TradeId and this test fails.

    """
    # Arrange
    order, venue_order_id = _add_accepted_order(exec_client, sinopac_equity)
    captured: list[TradeId] = []
    exec_client.generate_order_filled = MagicMock(
        side_effect=lambda *a, **k: captured.append(k["trade_id"]),
    )

    base_event = {
        "event_type": "stock_deal",
        "trade_id": venue_order_id.value,
        "seqno": "999999",  # identical across both fills -> would collide under seqno key
        "code": "2330",
        "action": "Buy",
        "price": 580.0,
        "quantity": 1000,
        "ts": 1709352601.0,
    }
    fill_1 = {**base_event, "ordno": "zZ9ab001", "exchange_seq": "X100"}
    fill_2 = {**base_event, "ordno": "zZ9ab002", "exchange_seq": "X200"}

    # Act
    exec_client._handle_deal_event(fill_1)
    exec_client._handle_deal_event(fill_2)

    # Assert -- the seqno key would be the same for both; the real key must differ.
    collision_key = TradeId(f"{venue_order_id.value}-999999")
    assert captured[0] != captured[1], "identical seqno must NOT cause a TradeId collision"
    assert captured[0] != collision_key
    assert captured[1] != collision_key


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
