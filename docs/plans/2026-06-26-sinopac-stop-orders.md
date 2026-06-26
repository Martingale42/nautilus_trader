# Sinopac Emulated Stop Orders + Capability-Doc Audit — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task, or superpowers:orchestrator-driven-development for a stateful, resumable multi-session pipeline.

**Goal:** Make NautilusTrader stop/conditional orders usable on the Sinopac venue by emulating them in-process (via the built-in `OrderEmulator`), fixing the current silent-drop of conditional orders, and bringing the integration doc back in line with the code.

**Architecture:** Sinopac/Shioaji has no native stop order type. Stops are provided by NautilusTrader's `OrderEmulator`: a strategy submits a stop/conditional order with `emulation_trigger=LAST_PRICE` (or `BID_ASK`); the emulator holds it locally, watches the market-data the Sinopac data client already streams, and on trigger releases a plain `MARKET`/`LIMIT` order into `SinopacExecutionClient._submit_order` (already supported). The adapter's only job is to (a) cleanly reject a *naked* (non-emulated) conditional order with a message pointing at emulation, (b) preserve order tags on released orders, and (c) document the pattern.

**Tech Stack:** Python (NautilusTrader adapter layer), pytest, the Sinopac live exec-tester example. No Rust changes.

**Spec:** `docs/superpowers/specs/2026-06-26-sinopac-stop-orders-design.md`

**Resume the design session:** `cd /home/cy/Code/MT5/nautilus_trader && claude --resume 6297df69-5502-4399-b9a6-c6126f9c89b7` (see the spec's section 0). This is the design/brainstorming session — distinct from the orchestrator implementation pipeline (`docs/sessions/orchestrator.md`).

---

## Setup note (one-time, read before Task 1)

The worktree extension is **already built** via `make build-debug` (Rust pyo3 + Cython).
All changes in this plan are **pure Python**, so no rebuild is ever needed — `.py` edits
are picked up live by the editable install.

**Always run tests with `uv run --no-sync`.** A plain `uv run` (no flag) re-syncs and
triggers a slow editable rebuild (minutes); `--no-sync` runs the suite in <1s. Confirm the
clean baseline once:

```bash
uv run --no-sync pytest tests/integration_tests/adapters/sinopac/ -q
```

Expected: the existing Sinopac suite passes (baseline: 107 passed in ~0.5s). If it fails
before any edits, stop and report — do not start Task 1 on a broken baseline.

---

## File structure

| File | Responsibility | Action |
|---|---|---|
| `nautilus_trader/adapters/sinopac/execution.py` | Add conditional-order-type set; replace silent-drop with `OrderRejected` | Modify |
| `tests/integration_tests/adapters/sinopac/test_execution.py` | Rejection tests + tag-preservation test | Modify |
| `examples/live/sinopac/sinopac_exec_tester.py` | `stop_market` + `bracket` sim scenarios | Modify |
| `docs/integrations/sinopac.md` | Capability-doc audit (order types, odd-lot, tags, stop section) | Modify |

---

### Task 1: Reject naked conditional orders in `_submit_order`

**Files:**
- Modify: `nautilus_trader/adapters/sinopac/execution.py`
- Test: `tests/integration_tests/adapters/sinopac/test_execution.py`

**Implementation:**

The supported price-type map covers only 3 of the 9 `OrderType` members; the other 6 are all stop/conditional types. Today `_submit_order` (line ~944) hits them with `self._log.error(...)` then `return` — **no `OrderRejected` event**, so the order hangs forever in NautilusTrader. Replace that with a real rejection that names emulation as the fix.

1. Add a module-level set next to `_NT_TO_SINOPAC_PRICE_TYPE` (currently `execution.py:169-173`):

```python
# The 6 OrderType members the venue cannot place directly. All are conditional
# (stop/trigger) types; on Sinopac they must be emulated via NautilusTrader's
# OrderEmulator (submit with `emulation_trigger`), which releases a plain
# MARKET/LIMIT order on trigger. See docs/integrations/sinopac.md.
_CONDITIONAL_ORDER_TYPES = frozenset(
    {
        OrderType.STOP_MARKET,
        OrderType.STOP_LIMIT,
        OrderType.MARKET_IF_TOUCHED,
        OrderType.LIMIT_IF_TOUCHED,
        OrderType.TRAILING_STOP_MARKET,
        OrderType.TRAILING_STOP_LIMIT,
    },
)
```

2. Replace the silent-drop branch (`execution.py:944-946`):

```python
if order.order_type not in _NT_TO_SINOPAC_PRICE_TYPE:
    if order.order_type in _CONDITIONAL_ORDER_TYPES:
        reason = (
            f"Sinopac has no native conditional orders ({order.order_type}); "
            "resubmit with emulation_trigger=LAST_PRICE or BID_ASK to use "
            "NautilusTrader order emulation"
        )
    else:
        # Defensive: no current OrderType reaches here, but a future enum
        # addition would otherwise be silently dropped.
        reason = f"Unsupported order type {order.order_type} for Sinopac"
    self.generate_order_rejected(
        strategy_id=order.strategy_id,
        instrument_id=instrument_id,
        client_order_id=order.client_order_id,
        reason=reason,
        ts_event=self._clock.timestamp_ns(),
    )
    return
```

`order.order_type` interpolates to an ASCII string (e.g. `OrderType.STOP_MARKET`), satisfying the repo's non-Latin lint hook. The rejection fires before `generate_order_submitted` (line ~948); `INITIALIZED → REJECTED` is a valid transition.

**Tests:** Required (public submit path, regression-critical — currently silent).

Add to `test_execution.py` (the `OrderFactory` import and helper are new; fixtures `exec_client`, `sinopac_equity`, `event_loop` already exist):

```python
from decimal import Decimal

import pytest
from nautilus_trader.common.factories import OrderFactory
from nautilus_trader.core.uuid import UUID4
from nautilus_trader.execution.messages import SubmitOrder
from nautilus_trader.model.enums import OrderSide, OrderType, TrailingOffsetType
from nautilus_trader.model.identifiers import StrategyId
from nautilus_trader.test_kit.stubs.identifiers import TestIdStubs


def _order_factory():
    from nautilus_trader.common.component import LiveClock

    return OrderFactory(
        trader_id=TestIdStubs.trader_id(),
        strategy_id=StrategyId("S-001"),
        clock=LiveClock(),
    )


def _naked_conditional(factory, instrument, order_type):
    qty = instrument.make_qty(2000)
    trig = instrument.make_price(590.0)
    lim = instrument.make_price(589.0)
    if order_type == OrderType.STOP_MARKET:
        return factory.stop_market(instrument.id, OrderSide.BUY, qty, trig)
    if order_type == OrderType.STOP_LIMIT:
        return factory.stop_limit(instrument.id, OrderSide.BUY, qty, lim, trig)
    if order_type == OrderType.MARKET_IF_TOUCHED:
        return factory.market_if_touched(instrument.id, OrderSide.BUY, qty, trig)
    if order_type == OrderType.LIMIT_IF_TOUCHED:
        return factory.limit_if_touched(instrument.id, OrderSide.BUY, qty, lim, trig)
    if order_type == OrderType.TRAILING_STOP_MARKET:
        return factory.trailing_stop_market(
            instrument.id, OrderSide.BUY, qty,
            trailing_offset=Decimal("1.0"),
            trailing_offset_type=TrailingOffsetType.PRICE,
        )
    if order_type == OrderType.TRAILING_STOP_LIMIT:
        return factory.trailing_stop_limit(
            instrument.id, OrderSide.BUY, qty,
            limit_offset=Decimal("1.0"),
            trailing_offset=Decimal("1.0"),
            trailing_offset_type=TrailingOffsetType.PRICE,
        )
    raise AssertionError(order_type)


@pytest.mark.parametrize(
    "order_type",
    [
        OrderType.STOP_MARKET,
        OrderType.STOP_LIMIT,
        OrderType.MARKET_IF_TOUCHED,
        OrderType.LIMIT_IF_TOUCHED,
        OrderType.TRAILING_STOP_MARKET,
        OrderType.TRAILING_STOP_LIMIT,
    ],
)
def test_naked_conditional_order_is_rejected_with_emulation_hint(
    event_loop, exec_client, sinopac_equity, order_type
):
    factory = _order_factory()
    order = _naked_conditional(factory, sinopac_equity, order_type)
    exec_client._cache.add_order(order)
    command = SubmitOrder(
        trader_id=order.trader_id,
        strategy_id=order.strategy_id,
        order=order,
        command_id=UUID4(),
        ts_init=0,
    )
    exec_client.generate_order_rejected = MagicMock()

    event_loop.run_until_complete(exec_client._submit_order(command))

    exec_client.generate_order_rejected.assert_called_once()
    reason = exec_client.generate_order_rejected.call_args.kwargs["reason"]
    assert "emulation_trigger" in reason
    exec_client._http_client.place_order.assert_not_called()
```

**Verification:**

Run: `uv run --no-sync pytest tests/integration_tests/adapters/sinopac/test_execution.py -k conditional -v`
Expected: 6 parametrized cases pass; `place_order` never called.

**Commit:**
```bash
git add nautilus_trader/adapters/sinopac/execution.py tests/integration_tests/adapters/sinopac/test_execution.py
git commit -m "Reject naked conditional orders with emulation hint

Sinopac has no native stop orders, so every conditional OrderType hit a
silent log-and-return in _submit_order, leaving the order hung in
NautilusTrader. Emit OrderRejected pointing the user at emulation_trigger."
```

---

### Task 2: Test that tags survive on a released (MARKET) order

**Files:**
- Test: `tests/integration_tests/adapters/sinopac/test_execution.py`

**Implementation:**

When an emulated `STOP_MARKET` triggers, the emulator releases a `MARKET` order carrying the *same* `order.tags`. This test proves the adapter still forwards Taiwan tags (e.g. margin) on that released order — i.e. "stop-loss that fires a margin sell" works. It exercises `_submit_order` directly with a tagged `MARKET` order (the released form), so no emulator is needed in the test.

```python
def test_market_order_preserves_margin_tag_through_submit(
    event_loop, exec_client, sinopac_equity
):
    factory = _order_factory()
    order = factory.market(
        sinopac_equity.id,
        OrderSide.SELL,
        sinopac_equity.make_qty(2000),
        time_in_force=TimeInForce.IOC,
        tags=[SinopacOrderTags(order_cond="MarginTrading").value],
    )
    exec_client._cache.add_order(order)
    command = SubmitOrder(
        trader_id=order.trader_id,
        strategy_id=order.strategy_id,
        order=order,
        command_id=UUID4(),
        ts_init=0,
    )

    event_loop.run_until_complete(exec_client._submit_order(command))

    exec_client._http_client.place_order.assert_awaited_once()
    kwargs = exec_client._http_client.place_order.call_args.kwargs
    assert kwargs["order_cond"] == SinopacOrderCond.MARGIN_TRADING
```

(`TimeInForce`, `SinopacOrderTags`, `SinopacOrderCond` are already imported in this test module.)

**Verification:**

Run: `uv run --no-sync pytest tests/integration_tests/adapters/sinopac/test_execution.py -k preserves_margin_tag -v`
Expected: passes; `place_order` called with `order_cond=MARGIN_TRADING`.

**Commit:**
```bash
git add tests/integration_tests/adapters/sinopac/test_execution.py
git commit -m "Test Taiwan tags survive on emulation-released market orders"
```

---

### Task 3: Add `stop_market` and `bracket` sim scenarios to the exec tester

**Files:**
- Modify: `examples/live/sinopac/sinopac_exec_tester.py`

**Implementation:**

Add two new env-selectable scenarios so the emulation path can be exercised against the sim gateway in dry-run. Both trade the equity (TSMC `2330`), so they reuse the fixed-id resolution path (no front-month logic).

1. Add constants (after `SCENARIO_FUTURES_OCTYPE`, line ~114) and extend the `SCENARIOS` tuple:

```python
SCENARIO_STOP_MARKET = "stop_market"
SCENARIO_BRACKET = "bracket"
SCENARIOS = (
    SCENARIO_COMMON,
    SCENARIO_INTRADAY_ODD,
    SCENARIO_MKP,
    SCENARIO_FUTURES_OCTYPE,
    SCENARIO_STOP_MARKET,
    SCENARIO_BRACKET,
)
```

2. Import `TriggerType` with the other enum imports:

```python
from nautilus_trader.model.enums import TriggerType
```

3. In `_build_order` (dispatch starts ~line 346), add a `stop_market` branch that returns an **emulated** stop. The emulator releases a MARKET on trigger; pick a trigger a little through the market so it can fire in sim:

```python
if self.config.scenario == SCENARIO_STOP_MARKET:
    trigger = self.instrument.make_price(float(quote.ask_price) + 1.0)
    return self.order_factory.stop_market(
        instrument_id=self.instrument_id,
        order_side=OrderSide.BUY,
        quantity=self.instrument.make_qty(2000),
        trigger_price=trigger,
        trigger_type=TriggerType.LAST_PRICE,
        emulation_trigger=TriggerType.LAST_PRICE,
    )
```

4. The bracket scenario submits an `OrderList`, not a single order, so handle it directly in `on_quote_tick` before the single-order path. Locate the `self.submit_order(order)` call (line ~323) and guard it:

```python
def on_quote_tick(self, quote: QuoteTick) -> None:
    if self._submitted:
        return
    self._submitted = True

    if self.config.scenario == SCENARIO_BRACKET:
        entry = self.instrument.make_price(float(quote.ask_price))
        bracket = self.order_factory.bracket(
            instrument_id=self.instrument_id,
            order_side=OrderSide.BUY,
            quantity=self.instrument.make_qty(2000),
            entry_price=entry,
            sl_trigger_price=self.instrument.make_price(float(entry) - 5.0),
            tp_price=self.instrument.make_price(float(entry) + 5.0),
            entry_order_type=OrderType.LIMIT,
            emulation_trigger=TriggerType.LAST_PRICE,
            time_in_force=TimeInForce.DAY,
        )
        self.log.info(f"Built bracket: {bracket}", LogColor.CYAN)
        if not self.config.dry_run:
            self.submit_order_list(bracket)
        return

    order = self._build_order(quote)
    ...  # existing single-order logging + submit path unchanged
```

Add a `self._submitted = False` guard in `__init__` if the existing code does not already gate on first quote (check `on_quote_tick`; the existing strategy submits "on the first quote received" per its docstring — reuse that gate rather than adding a second).

> NOTE: bracket/stop emulation requires the node's `OrderEmulator`, which is active by default in a live node; no extra wiring needed. In `dry_run` the orders are built and logged but not submitted.

**Verification:**

Run (no live gateway needed — exercises construction + dispatch):
```bash
SINOPAC_EXEC_SCENARIO=stop_market SINOPAC_EXEC_DRY_RUN=1 \
  uv run --no-sync python -c "import examples.live.sinopac.sinopac_exec_tester as t; print(t.SCENARIOS)"
```
Expected: prints the 6-scenario tuple including `stop_market` and `bracket` with no import error.

Manual (optional, against sim gateway): run the tester with `SINOPAC_EXEC_SCENARIO=stop_market SINOPAC_EXEC_DRY_RUN=1` and confirm the log shows the emulated stop being built.

**Commit:**
```bash
git add examples/live/sinopac/sinopac_exec_tester.py
git commit -m "Add emulated stop_market and bracket scenarios to Sinopac exec tester"
```

---

### Task 4: Capability-doc audit of `docs/integrations/sinopac.md`

**Files:**
- Modify: `docs/integrations/sinopac.md`

**Implementation:**

Bring the doc in line with the code (drift confirmed in the spec §4.1). Make these four edits.

**(a) Order types** — extend the matrix (currently L181-184) and add a stop subsection:

```markdown
| Order Type        | Stocks | Futures | Options | Notes |
|-------------------|--------|---------|---------|-------|
| `MARKET`          | ✓      | ✓       | ✓       | Coerced to IOC if TIF is DAY/GTC. |
| `LIMIT`           | ✓      | ✓       | ✓       | Price snapped to the tick grid. |
| `MARKET_TO_LIMIT` | ✗      | ✓       | ✓       | Range-market (MKP); stock MKP rejected locally. |

#### Stop / conditional orders (emulated only)

Sinopac/Shioaji has no native stop or trigger order type. `STOP_MARKET`,
`STOP_LIMIT`, `MARKET_IF_TOUCHED`, `LIMIT_IF_TOUCHED`, and the trailing-stop
variants are supported **only** through NautilusTrader's order emulation: submit
with `emulation_trigger=TriggerType.LAST_PRICE` (recommended for TWSE/TAIFEX) or
`TriggerType.BID_ASK`. The emulator holds the order in-process, watches the
data the Sinopac data client streams, and releases a plain `MARKET`/`LIMIT`
order to the venue on trigger. A conditional order submitted **without**
`emulation_trigger` is rejected locally.

Limitation: the trigger lives in the NautilusTrader process. If the process
stops, an emulated stop does not fire during the outage. Back the cache with
Redis so emulated orders survive a restart.
```

**(b) Odd-lot** — replace the incorrect warning block (currently L208-213) with:

```markdown
:::note
`IntradayOdd` (盤中零股, 09:00–13:30) is supported: attach
`SinopacOrderTags(order_lot="IntradayOdd")` to the order's `tags`. Quantities are
in shares (1–999), and the order must be `LIMIT` + `DAY`. Post-market `Odd`
(盤後零股) and `Fixing` (定盤) lots remain out of scope (backlog item B3).
:::
```

Also fix the prose at L193-197 that claims `_submit_order` "does not send an
`order_lot` field" — it always sends `order_lot` (defaulting to `Common`).

**(c) Taiwan order tags** — add a new section documenting the tag capabilities
that are implemented but currently undocumented:

```markdown
### Taiwan order tags

Venue parameters with no native Nautilus field ride on `order.tags` via
`SinopacOrderTags`. See the canonical capability matrix in the adapter package
docstring (`nautilus_trader/adapters/sinopac/__init__.py`).

| Tag field        | Values                                   | Applies to |
|------------------|------------------------------------------|------------|
| `order_lot`      | `Common`, `IntradayOdd`                  | Stocks     |
| `order_cond`     | `Cash`, `MarginTrading`, `ShortSelling`  | Stocks     |
| `daytrade_short` | `True` / `False` (requires `Cash`)       | Stocks     |
| `octype`         | `Auto`, `New`, `Cover`, `DayTrade`       | Futures/Options |
```

**(d) Single source of truth** — where the integration doc and the `__init__.py`
matrix overlap, link to `__init__.py` as canonical rather than restating, to
prevent the drift this task is fixing from recurring.

**Verification:**

Run: `grep -n "emulation_trigger\|IntradayOdd\|order_cond\|octype" docs/integrations/sinopac.md`
Expected: the new stop section, corrected odd-lot note, and tag matrix are present.

Manual: re-read the edited sections against `execution.py` (`_NT_TO_SINOPAC_PRICE_TYPE`, `_resolve_validated_tags`, tag plumbing at lines ~1004-1009) — every claim must match code.

**Commit:**
```bash
git add docs/integrations/sinopac.md
git commit -m "Audit Sinopac integration doc against adapter code

Document emulated stop/conditional orders, correct the stale odd-lot
'not implemented' claim (IntradayOdd works via tags), and add the
order_cond/daytrade_short/octype tag matrix that the code already supports."
```

---

## Final verification

```bash
uv run --no-sync pytest tests/integration_tests/adapters/sinopac/ -q
```
Expected: full Sinopac suite green (existing + new tests).

Acceptance criteria (spec §6): naked conditional orders rejected with hint ✓; emulation release path validated ✓; tags preserved ✓; exec-tester scenarios added ✓; doc audited ✓.
