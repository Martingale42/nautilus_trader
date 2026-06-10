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

from unittest.mock import AsyncMock
from unittest.mock import MagicMock

import pytest

from nautilus_trader.adapters.sinopac.config import SinopacExecClientConfig
from nautilus_trader.adapters.sinopac.execution import SinopacExecutionClient
from nautilus_trader.adapters.sinopac.execution import _coid_token
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


def test_async_new_failure_on_accepted_order_rejects(exec_client, sinopac_equity):
    """
    Reject an ACCEPTED order when the venue rejection arrives asynchronously.

    This is SINOPAC-04's dominant path: the gateway returns HTTP 200 +
    PendingSubmit for an off-tick/over-band order, `_submit_order` marks it
    ACCEPTED, and the venue rejection surfaces LATER as a `New` order event with
    op_code != "00". ACCEPTED -> REJECTED is a legal NT transition, so the order
    must end REJECTED (no lingering ACCEPTED ghost) and the mapping is dropped.

    """
    # Arrange
    order, venue_order_id = _add_accepted_order(exec_client, sinopac_equity)
    assert order.status == OrderStatus.ACCEPTED

    exec_client.generate_order_rejected = MagicMock()

    async_reject = {
        "event_type": "stock_order",
        "op_type": "New",
        "op_code": "88",  # off-tick venue rejection (price tick error)
        "op_msg": "價格檔數錯誤",
        "order_id": venue_order_id.value,
        "code": "2330",
    }

    # Act (must not raise)
    exec_client._handle_order_status_event(async_reject)

    # Assert -- the late async rejection drives ACCEPTED -> REJECTED.
    exec_client.generate_order_rejected.assert_called_once()
    call_kwargs = exec_client.generate_order_rejected.call_args.kwargs
    assert call_kwargs["reason"] == "價格檔數錯誤"
    # Mapping dropped: the order is no longer working at the venue.
    assert venue_order_id.value not in exec_client._trade_id_to_client_order_id


def test_new_failure_on_filled_order_is_ignored(exec_client, sinopac_equity):
    """
    Ignore a `New` failure that arrives after the order is FILLED.

    FILLED -> REJECTED is an illegal NT transition that would panic the Rust
    state machine, so the residual guard must still suppress it (only ACCEPTED
    and SUBMITTED orders are rejectable on an async `New` failure).

    """
    # Arrange
    from nautilus_trader.model.identifiers import VenueOrderId

    venue_order_id = VenueOrderId("T0003")
    order = TestExecStubs.make_filled_order(
        instrument=sinopac_equity,
        order_side=OrderSide.BUY,
    )
    exec_client._cache.add_order(order)
    exec_client._trade_id_to_client_order_id[venue_order_id.value] = order.client_order_id.value
    assert order.status == OrderStatus.FILLED

    exec_client.generate_order_rejected = MagicMock()

    late_failure = {
        "event_type": "stock_order",
        "op_type": "New",
        "op_code": "99",
        "op_msg": "rejected after fill",
        "order_id": venue_order_id.value,
        "code": "2330",
    }

    # Act (must not raise)
    exec_client._handle_order_status_event(late_failure)

    # Assert -- the guard suppresses the illegal FILLED -> REJECTED transition.
    exec_client.generate_order_rejected.assert_not_called()
    assert order.status == OrderStatus.FILLED


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
    exec_client._http_client.place_order = AsyncMock(side_effect=TimeoutError())

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


# -- I2: reconciliation convergence after a timed-out submit (documents real behavior) --------------


@pytest.mark.asyncio
async def test_p3_reconciliation_with_token_adopts_timed_out_order(
    exec_client,
    sinopac_equity,
):
    """
    After a place_order timeout, reconciliation via list_trades recovers the
    original client_order_id through the custom_field token round-trip, enabling
    NT to adopt the SUBMITTED order instead of creating an external duplicate.

    This is the BL-1 fix: the 6-char token stored in custom_field at submit time
    is echoed back by list_trades. The adapter recomputes the token for each
    cached non-closed order and matches, recovering the real client_order_id.

    """
    from nautilus_trader.execution.messages import GenerateOrderStatusReports
    from nautilus_trader.model.enums import OrderStatus as NTOrderStatus
    from nautilus_trader.model.identifiers import ClientOrderId
    from nautilus_trader.model.identifiers import VenueOrderId

    # Arrange: a local order that timed out on submit -> SUBMITTED, no venue mapping.
    order = TestExecStubs.make_submitted_order(
        instrument=sinopac_equity,
        order_side=OrderSide.BUY,
        quantity=sinopac_equity.make_qty(2000),
        price=sinopac_equity.make_price(580.0),
    )
    exec_client._cache.add_order(order)
    assert order.status == OrderStatus.SUBMITTED
    # Mapping was never populated (timeout never returned a trade_id).
    assert order.venue_order_id is None
    assert "T9999" not in exec_client._trade_id_to_client_order_id

    # Compute the token that _submit_order would have set
    token = _coid_token(order.client_order_id.value)

    # The venue actually holds the order as a working (Submitted) trade,
    # with the custom_field token round-tripped.
    exec_client._http_client.list_trades = AsyncMock(
        return_value=[
            {
                "trade_id": "T9999",
                "code": "2330",
                "status": "Submitted",
                "action": "Buy",
                "price_type": "LMT",
                "order_type": "ROD",
                "quantity": 2000,
                "filled_qty": 0,
                "price": 580.0,
                "custom_field": token,
            },
        ],
    )

    command = GenerateOrderStatusReports(
        instrument_id=None,
        start=None,
        end=None,
        open_only=False,
        command_id=TestIdStubs.uuid(),
        ts_init=0,
    )

    # Act
    reports = await exec_client.generate_order_status_reports(command)

    # Assert -- reconciliation recovered the REAL client_order_id via token.
    assert len(reports) == 1
    report = reports[0]
    assert report.venue_order_id == VenueOrderId("T9999")
    assert report.order_status == NTOrderStatus.ACCEPTED  # gateway "Submitted" -> ACCEPTED

    # The report carries the REAL client_order_id (not the synthetic one).
    # This is the BL-1 fix: NT can match this to the cached SUBMITTED order
    # and adopt it, attaching venue_order_id and advancing state.
    assert report.client_order_id == order.client_order_id
    assert report.client_order_id != ClientOrderId("SINOPAC-T9999")

    # The mapping was backfilled by _resolve_client_order_id for future events.
    assert exec_client._trade_id_to_client_order_id["T9999"] == order.client_order_id.value


@pytest.mark.asyncio
async def test_p3_reconciliation_without_token_falls_back_to_synthetic(
    exec_client,
    sinopac_equity,
):
    """
    When custom_field is absent (e.g. orders placed before token feature),
    reconciliation falls back to the synthetic SINOPAC-{trade_id} id.
    This is the pre-BL-1 behavior, preserved for backward compatibility.

    """
    from nautilus_trader.execution.messages import GenerateOrderStatusReports
    from nautilus_trader.model.identifiers import ClientOrderId

    # No local submitted order with matching token in cache.
    exec_client._http_client.list_trades = AsyncMock(
        return_value=[
            {
                "trade_id": "T8888",
                "code": "2330",
                "status": "Submitted",
                "action": "Buy",
                "price_type": "LMT",
                "order_type": "ROD",
                "quantity": 1000,
                "filled_qty": 0,
                "price": 580.0,
                # No custom_field -> fallback to synthetic
            },
        ],
    )

    command = GenerateOrderStatusReports(
        instrument_id=None,
        start=None,
        end=None,
        open_only=False,
        command_id=TestIdStubs.uuid(),
        ts_init=0,
    )

    # Act
    reports = await exec_client.generate_order_status_reports(command)

    # Assert -- synthetic client_order_id (unchanged pre-BL-1 behavior).
    assert len(reports) == 1
    report = reports[0]
    assert report.client_order_id == ClientOrderId("SINOPAC-T8888")


# -- BL-1: custom_field token round-trip for timed-out order adoption ----------------------------


def test_coid_token_deterministic():
    """The token must be deterministic (restart-safe)."""
    coid = "O-20260608-001-000-001"
    assert _coid_token(coid) == _coid_token(coid)


def test_coid_token_length_and_ascii():
    """Token must be exactly 6 ASCII chars (fits ConStrAsciiMax6)."""
    token = _coid_token("any-client-order-id-value")
    assert len(token) == 6
    assert all(c.isalnum() for c in token)


def test_coid_token_different_inputs_differ():
    """Different client_order_ids should produce different tokens (low collision)."""
    t1 = _coid_token("O-20260608-001-000-001")
    t2 = _coid_token("O-20260608-001-000-002")
    assert t1 != t2


@pytest.mark.asyncio
async def test_bl1_submit_order_sends_custom_field_token(exec_client, sinopac_equity):
    """
    _submit_order must send the token as custom_field in the HTTP place_order call.
    """
    order = TestExecStubs.limit_order(
        instrument=sinopac_equity,
        order_side=OrderSide.BUY,
        quantity=sinopac_equity.make_qty(2000),
        price=sinopac_equity.make_price(580.0),
    )
    exec_client._cache.add_order(order)
    exec_client.generate_order_submitted = MagicMock()
    exec_client.generate_order_accepted = MagicMock()

    exec_client._http_client.place_order = AsyncMock(
        return_value={
            "trade_id": "T-NEW",
            "code": "2330",
            "action": "Buy",
            "status": "PendingSubmit",
        },
    )

    command = SubmitOrder(
        trader_id=order.trader_id,
        strategy_id=order.strategy_id,
        order=order,
        command_id=TestIdStubs.uuid(),
        ts_init=0,
    )

    await exec_client._submit_order(command)

    # Verify custom_field was passed
    call_kwargs = exec_client._http_client.place_order.call_args.kwargs
    expected_token = _coid_token(order.client_order_id.value)
    assert call_kwargs["custom_field"] == expected_token


def test_bl1_order_status_event_with_token_resolves_timed_out_order(
    exec_client,
    sinopac_equity,
):
    """
    An order-status WS event carrying custom_field token resolves a timed-out
    order (no trade_id mapping) back to the original client_order_id.
    """
    # Create a submitted order (simulating timeout: no venue mapping).
    order = TestExecStubs.make_submitted_order(
        instrument=sinopac_equity,
        order_side=OrderSide.BUY,
        quantity=sinopac_equity.make_qty(2000),
        price=sinopac_equity.make_price(580.0),
    )
    exec_client._cache.add_order(order)

    token = _coid_token(order.client_order_id.value)

    # Simulate a "New" op_code="00" (accepted) event with the token but no
    # trade_id mapping. "New" with success is a no-op in the handler (comment
    # in the code: "already handled in _submit_order"), but crucially the
    # client_order_id resolution runs BEFORE the op_type switch, so the mapping
    # is backfilled. We verify the backfill.
    event = {
        "event_type": "stock_order",
        "op_type": "New",
        "op_code": "00",
        "op_msg": "",
        "order_id": "T-VENUE-001",
        "code": "2330",
        "custom_field": token,
    }

    exec_client._handle_order_status_event(event)

    # Mapping was backfilled by _resolve_client_order_id during the lookup
    assert exec_client._trade_id_to_client_order_id["T-VENUE-001"] == order.client_order_id.value

    # Now verify a subsequent cancel works using the backfilled mapping
    exec_client.generate_order_canceled = MagicMock()
    cancel_event = {
        "event_type": "stock_order",
        "op_type": "Cancel",
        "op_code": "00",
        "op_msg": "",
        "order_id": "T-VENUE-001",
        "code": "2330",
        "custom_field": token,
    }
    exec_client._handle_order_status_event(cancel_event)

    exec_client.generate_order_canceled.assert_called_once()
    call_kwargs = exec_client.generate_order_canceled.call_args.kwargs
    assert call_kwargs["client_order_id"].value == order.client_order_id.value


def test_bl1_deal_event_resolves_via_backfilled_mapping(
    exec_client,
    sinopac_equity,
):
    """
    After an order-status event backfills the mapping via token, a subsequent
    deal event resolves via the fast-path (direct mapping) and generates a fill.
    """
    # Create a submitted order, no venue mapping (timeout scenario).
    order = TestExecStubs.make_submitted_order(
        instrument=sinopac_equity,
        order_side=OrderSide.BUY,
        quantity=sinopac_equity.make_qty(2000),
        price=sinopac_equity.make_price(580.0),
    )
    exec_client._cache.add_order(order)

    token = _coid_token(order.client_order_id.value)
    trade_id = "T-VENUE-002"

    # Simulate order-status event to backfill mapping
    exec_client.generate_order_canceled = MagicMock()
    event_order = {
        "event_type": "stock_order",
        "op_type": "New",
        "op_code": "00",
        "op_msg": "",
        "order_id": trade_id,
        "code": "2330",
        "custom_field": token,
    }
    exec_client._handle_order_status_event(event_order)

    # Mapping is now populated
    assert trade_id in exec_client._trade_id_to_client_order_id

    # Now a deal event arrives without custom_field (deals may not carry it)
    exec_client.generate_order_filled = MagicMock()
    deal_event = {
        "event_type": "stock_deal",
        "trade_id": trade_id,
        "seqno": "100",
        "ordno": "ABC001",
        "exchange_seq": "E999",
        "code": "2330",
        "action": "Buy",
        "price": 580.0,
        "quantity": 2000,
        "ts": 1709352601.0,
    }

    exec_client._handle_deal_event(deal_event)

    # Fill was generated using the correct client_order_id
    exec_client.generate_order_filled.assert_called_once()
    call_kwargs = exec_client.generate_order_filled.call_args.kwargs
    assert call_kwargs["client_order_id"].value == order.client_order_id.value


@pytest.mark.asyncio
async def test_bl1_restart_adopt_via_recomputed_hash(exec_client, sinopac_equity):
    """
    After restart, in-memory mapping is empty. Reconciliation recomputes the
    token hash from cached orders and still resolves the timed-out order.
    """
    from nautilus_trader.execution.messages import GenerateOrderStatusReports
    from nautilus_trader.model.identifiers import ClientOrderId

    # Simulate post-restart: submitted order in cache, mapping cleared.
    order = TestExecStubs.make_submitted_order(
        instrument=sinopac_equity,
        order_side=OrderSide.BUY,
        quantity=sinopac_equity.make_qty(2000),
        price=sinopac_equity.make_price(580.0),
    )
    exec_client._cache.add_order(order)
    exec_client._trade_id_to_client_order_id.clear()

    token = _coid_token(order.client_order_id.value)

    exec_client._http_client.list_trades = AsyncMock(
        return_value=[
            {
                "trade_id": "T-RESTART",
                "code": "2330",
                "status": "Submitted",
                "action": "Buy",
                "price_type": "LMT",
                "order_type": "ROD",
                "quantity": 2000,
                "filled_qty": 0,
                "price": 580.0,
                "custom_field": token,
            },
        ],
    )

    command = GenerateOrderStatusReports(
        instrument_id=None,
        start=None,
        end=None,
        open_only=False,
        command_id=TestIdStubs.uuid(),
        ts_init=0,
    )

    reports = await exec_client.generate_order_status_reports(command)

    assert len(reports) == 1
    report = reports[0]
    # Recovered the REAL client_order_id via hash recompute
    assert report.client_order_id == order.client_order_id
    assert report.client_order_id != ClientOrderId("SINOPAC-T-RESTART")


def test_bl1_external_order_no_token_no_mapping_returns_none(exec_client):
    """
    _resolve_client_order_id returns None for truly external orders
    (no mapping, no token).
    """
    result = exec_client._resolve_client_order_id("UNKNOWN-TRADE", None)
    assert result is None

    result2 = exec_client._resolve_client_order_id("UNKNOWN-TRADE", "")
    assert result2 is None


# -- Task 3.2: exec client establishes the shared WS independently ---------------------------------


@pytest.fixture
def stateful_ws_client():
    """
    Stub a pyo3 WS client whose is_connected tracks connect/disconnect calls.
    """
    stub = MagicMock(spec=pyo3_sinopac.SinopacWebSocketClient)
    state = {"connected": False}

    async def _connect(*args, **kwargs):
        state["connected"] = True

    async def _disconnect(*args, **kwargs):
        state["connected"] = False

    async def _wait_until_active(*args, **kwargs):
        return None

    stub.connect = AsyncMock(side_effect=_connect)
    stub.disconnect = AsyncMock(side_effect=_disconnect)
    stub.wait_until_active = AsyncMock(side_effect=_wait_until_active)
    stub.is_connected = MagicMock(side_effect=lambda: state["connected"])
    return stub


def _build_exec_client(event_loop, instrument, ws_client, ws_dispatcher):
    clock = LiveClock()
    trader_id = TestIdStubs.trader_id()
    msgbus = MessageBus(trader_id, clock)
    cache = TestComponentStubs.cache()
    cache.add_instrument(instrument)

    http_client = MagicMock(spec=pyo3_sinopac.SinopacHttpClient)
    http_client.place_order = AsyncMock()
    http_client.account_balance = AsyncMock(return_value={"balance": 1_000_000.0})

    provider = MagicMock(spec=SinopacInstrumentProvider)
    provider.initialize = AsyncMock()
    provider.instruments_pyo3 = MagicMock(return_value=[])

    return SinopacExecutionClient(
        loop=event_loop,
        client=http_client,
        ws_client=ws_client,
        msgbus=msgbus,
        cache=cache,
        clock=clock,
        instrument_provider=provider,
        config=SinopacExecClientConfig(),
        name=None,
        ws_dispatcher=ws_dispatcher,
    )


@pytest.mark.asyncio
async def test_exec_only_connect_establishes_ws_and_dispatches_order_event(
    event_loop,
    sinopac_equity,
    stateful_ws_client,
):
    """
    An exec-only node (no data client) must connect the shared WS itself and then
    receive an order event dispatched through that WS.
    """
    from nautilus_trader.adapters.sinopac.factories import _WsDispatcher

    dispatcher = _WsDispatcher(stateful_ws_client)
    client = _build_exec_client(event_loop, sinopac_equity, stateful_ws_client, dispatcher)

    # Act -- run the exec client's connect sequence directly.
    await client._connect()

    # Assert -- the exec client established the WS with no data client present.
    assert stateful_ws_client.is_connected()
    stateful_ws_client.connect.assert_awaited_once()

    # A dispatched order event reaches the exec client's order-event path.
    order, venue_order_id = _add_accepted_order(client, sinopac_equity)
    client.generate_order_canceled = MagicMock()
    cancel_event = {
        "event_type": "stock_order",
        "op_type": "Cancel",
        "op_code": "00",
        "op_msg": "",
        "order_id": venue_order_id.value,
        "code": "2330",
    }
    dispatcher.dispatch(cancel_event)

    client.generate_order_canceled.assert_called_once()


@pytest.mark.asyncio
async def test_data_disconnect_leaves_ws_up_for_exec(
    event_loop,
    sinopac_equity,
    stateful_ws_client,
):
    """
    With both clients registered, the data client releasing must not tear down
    the shared WS while the exec client is still registered.
    """
    from nautilus_trader.adapters.sinopac.factories import _WsDispatcher

    dispatcher = _WsDispatcher(stateful_ws_client)
    client = _build_exec_client(event_loop, sinopac_equity, stateful_ws_client, dispatcher)

    # Exec connects (registers + establishes WS).
    await client._connect()
    assert stateful_ws_client.is_connected()

    # Simulate a data client sharing the same WS: register a second handler and
    # establish (idempotent no-op), then have it release.
    def data_handler(msg: object) -> None:
        pass

    dispatcher.register(data_handler)
    await dispatcher.ensure_connected(instruments=[])

    dispatcher.unregister(data_handler)
    await dispatcher.release()

    # Exec is still registered -> WS must remain connected.
    assert stateful_ws_client.is_connected(), "data release severed the exec event stream"
    stateful_ws_client.disconnect.assert_not_called()

    # Now exec disconnects -> WS finally tears down.
    await client._disconnect()
    assert not stateful_ws_client.is_connected()
    stateful_ws_client.disconnect.assert_awaited_once()


# -- Task 3.3: defensive submit-status check + reconnect reconciliation ----------------------------


@pytest.mark.asyncio
async def test_submit_order_rejects_on_synchronous_failed_status(exec_client, sinopac_equity):
    """
    Reject (defensively) when the gateway returns HTTP 200 with a Failed status.

    This second line of defense covers a synchronously-rejecting or stale gateway
    that echoes a terminal `Status.Failed` rather than raising or returning 422.
    The trade_id mapping must NOT be populated for such a non-working order.
    """
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

    # Gateway returns 200 but with a terminal Failed status (tail-split robust to
    # the "Status."/"OrderStatus." prefix).
    exec_client._http_client.place_order = AsyncMock(
        return_value={"trade_id": "T-FAIL", "code": "2330", "status": "Status.Failed"},
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

    # Assert -- defensive rejection, no accept, no mapping for the dead order.
    exec_client.generate_order_rejected.assert_called_once()
    exec_client.generate_order_accepted.assert_not_called()
    assert "T-FAIL" not in exec_client._trade_id_to_client_order_id


@pytest.mark.asyncio
async def test_submit_order_accepts_on_pending_submit_status(exec_client, sinopac_equity):
    """
    Accept normally when the gateway returns the live PendingSubmit status.

    The async rejection (if any) surfaces later via the order-event path, not the
    defensive submit check.
    """
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

    exec_client._http_client.place_order = AsyncMock(
        return_value={
            "trade_id": "T-OK",
            "code": "2330",
            "status": "OrderStatus.PendingSubmit",
        },
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

    # Assert -- accepted, mapping populated.
    exec_client.generate_order_accepted.assert_called_once()
    exec_client.generate_order_rejected.assert_not_called()
    assert exec_client._trade_id_to_client_order_id["T-OK"] == order.client_order_id.value


def test_reconnected_event_schedules_reconciliation(exec_client):
    """
    A reconnected WS sentinel must schedule the reconnect reconciliation task.
    """
    created: list[object] = []

    # Spy on the loop so the scheduled coroutine is captured (and closed) rather
    # than left pending.
    def _capture_task(coro, *args, **kwargs):
        created.append(coro)
        coro.close()  # avoid "never awaited" warning

    exec_client._loop = MagicMock()
    exec_client._loop.create_task = MagicMock(side_effect=_capture_task)

    # Act -- the synthetic reconnected dict flows through the order-event entry.
    exec_client._handle_order_event({"event": "reconnected"})

    # Assert -- exactly one reconcile coroutine was scheduled.
    assert len(created) == 1
    assert exec_client._loop.create_task.call_count == 1


@pytest.mark.asyncio
async def test_reconcile_after_reconnect_sends_mass_status(exec_client):
    """
    Reconnect reconciliation must regenerate a mass status and send it to the
    engine's mass-status reconciliation entrypoint.
    """
    from nautilus_trader.execution.reports import ExecutionMassStatus

    mass_status = ExecutionMassStatus(
        client_id=exec_client.id,
        account_id=exec_client.account_id,
        venue=exec_client.venue,
        report_id=TestIdStubs.uuid(),
        ts_init=0,
    )
    exec_client.generate_mass_status = AsyncMock(return_value=mass_status)
    exec_client._send_mass_status_report = MagicMock()

    # Act
    await exec_client._reconcile_after_reconnect()

    # Assert -- regenerated and routed to the engine's reconciliation endpoint.
    exec_client.generate_mass_status.assert_awaited_once()
    exec_client._send_mass_status_report.assert_called_once_with(mass_status)


# -- Task 3.3: data client tolerates exec-bound dicts without WARNING spam (A7) --------------------
#
# `Component._log` is a read-only Cython slot, so we invoke the unbound
# `_handle_msg` with a mock `self` (carrying a mock `_log`) -- the dict branches
# under test only touch `self._log`/`self._handle_data`.


def test_data_client_logs_reconnected_at_info_not_warning():
    """
    The data client must log the reconnected sentinel at INFO, never WARNING.
    """
    from nautilus_trader.adapters.sinopac.data import SinopacDataClient

    mock_self = MagicMock()

    SinopacDataClient._handle_msg(mock_self, {"event": "reconnected"})

    mock_self._log.info.assert_called_once()
    mock_self._log.warning.assert_not_called()


def test_data_client_drops_order_event_dict_without_warning():
    """
    Order/fill dicts (meant for the exec client) must be dropped at DEBUG, never
    WARNING -- otherwise the shared WS spams a warning per order event (A7).
    """
    from nautilus_trader.adapters.sinopac.data import SinopacDataClient

    mock_self = MagicMock()

    order_event = {
        "event_type": "stock_order",
        "op_type": "New",
        "op_code": "00",
        "order_id": "T0001",
        "code": "2330",
    }
    SinopacDataClient._handle_msg(mock_self, order_event)

    mock_self._log.warning.assert_not_called()
    mock_self._log.debug.assert_called_once()
    mock_self._handle_data.assert_not_called()


# -- Task 3.4: consume the gateway-reported filled_qty in order status reports ---------------------


@pytest.mark.asyncio
async def test_order_status_report_uses_gateway_filled_qty(exec_client, sinopac_equity):
    """
    A PartFilled trade must report the gateway-reported filled_qty (shares, D1).
    """
    from nautilus_trader.execution.messages import GenerateOrderStatusReports

    exec_client._http_client.list_trades = AsyncMock(
        return_value=[
            {
                "trade_id": "T-PART",
                "code": "2330",
                "status": "PartFilled",
                "action": "Buy",
                "price_type": "LMT",
                "order_type": "ROD",
                "quantity": 2000,
                "filled_qty": 1000,
                "price": 580.0,
            },
        ],
    )

    command = GenerateOrderStatusReports(
        instrument_id=None,
        start=None,
        end=None,
        open_only=False,
        command_id=TestIdStubs.uuid(),
        ts_init=0,
    )

    # Act
    reports = await exec_client.generate_order_status_reports(command)

    # Assert -- filled_qty echoes the gateway value (in shares).
    assert len(reports) == 1
    assert reports[0].filled_qty == sinopac_equity.make_qty(1000)


@pytest.mark.asyncio
async def test_order_status_report_falls_back_to_zero_when_filled_qty_missing(
    exec_client,
    sinopac_equity,
):
    """
    A missing filled_qty must fall back to 0 (the warned-incomplete path).

    The warning itself is emitted through NT's Rust logger, which does not
    propagate to pytest's caplog; the observable contract under test is the
    explicit 0 fallback that distinguishes "missing" from a real reported value.
    """
    from nautilus_trader.execution.messages import GenerateOrderStatusReports

    exec_client._http_client.list_trades = AsyncMock(
        return_value=[
            {
                "trade_id": "T-OLD",
                "code": "2330",
                "status": "Submitted",
                "action": "Buy",
                "price_type": "LMT",
                "order_type": "ROD",
                "quantity": 1000,
                "price": 580.0,
                # No filled_qty key -> older gateway
            },
        ],
    )

    command = GenerateOrderStatusReports(
        instrument_id=None,
        start=None,
        end=None,
        open_only=False,
        command_id=TestIdStubs.uuid(),
        ts_init=0,
    )

    # Act
    reports = await exec_client.generate_order_status_reports(command)

    # Assert -- explicit 0 fallback (not a silently-assumed default).
    assert len(reports) == 1
    assert reports[0].filled_qty == sinopac_equity.make_qty(0)
