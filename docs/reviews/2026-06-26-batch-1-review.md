# Code Review — Sinopac Stop Orders, Batch 1 (Tasks 1-2)

- **Date:** 2026-06-26
- **Reviewer role:** Code Reviewer (Sinopac stop-orders orchestrator pipeline)
- **Scope:** commits `9257bdae83` (reject naked conditional orders) and `98db183579` (tag-preservation test)
- **Plan:** `docs/plans/2026-06-26-sinopac-stop-orders.md` (Tasks 1-2)
- **Spec:** `docs/superpowers/specs/2026-06-26-sinopac-stop-orders-design.md`

## Verdict: APPROVED

> **Updated 2026-06-26 after fix commit `b349a7564a`.** Original verdict was
> CHANGES REQUESTED; both Important findings (I-1, I-2) and both Minor findings
> (M-1, M-2) are now resolved. See the **Fix Verification** section at the bottom
> for the per-finding evidence. The original review text below is preserved
> unchanged for the record.

The core behavior is correct and well-targeted: all six conditional `OrderType`
members now reach `generate_order_rejected` with an actionable `emulation_trigger`
hint instead of the old silent `log + return`, and the `INITIALIZED -> REJECTED`
transition is valid and fires before any `generate_order_submitted`. Two issues
block a clean merge: a ruff `I001` import-ordering failure in the test file (the
repo's own lint gate rejects it), and a non-actionable order-type rendering in the
rejection `reason` (`(3)` instead of `(STOP_MARKET)`) that the test cannot catch.
Both are small, mechanical fixes.

---

## Findings

### Important

**I-1. Import-ordering lint failure (ruff `I001`) in the test file.**
`[tests/integration_tests/adapters/sinopac/test_execution.py:24]`
The new `from nautilus_trader.common.factories import OrderFactory` was inserted
between two `nautilus_trader.adapters.sinopac.execution` imports (lines 23 and 25),
breaking alphabetical grouping. `uv run --no-sync ruff check` reports
`I001 Import block is un-sorted or un-formatted`. The repo's `[tool.ruff.lint]`
selects `"I"`, and `[tool.ruff.lint.per-file-ignores]` for `tests/**/*.py` is only
`["S101","S105","S106"]` — `I001` is NOT exempt for tests. The pre-commit `ruff`
hook (run with `--fix`) would rewrite this file, so the commit as-is is not
lint-clean. Fix: move the `OrderFactory` import into the `nautilus_trader.common.*`
group (after `common.component`, before `common.factories` sort position) or run
`ruff check --fix`.

**I-2. Rejection `reason` renders the order type as an opaque integer, not its name.**
`[nautilus_trader/adapters/sinopac/execution.py:962]` (and the defensive branch at
`:969`)
`f"{order.order_type}"` evaluates to `'3'`, not `'STOP_MARKET'` — the model enum's
`__str__`/`__format__` returns the integer value. The emitted reason therefore reads:
`"Sinopac has no native conditional orders (3); resubmit with emulation_trigger=..."`.
The `emulation_trigger` guidance is present and actionable, but the order-type
identifier `(3)` is not human-readable, so the reason is less actionable than the
plan intends. The plan's claim (plan line ~100: "`order.order_type` interpolates to
an ASCII string, e.g. `OrderType.STOP_MARKET`") is factually wrong — verified
against both `str(OrderType.STOP_MARKET) == '3'` and a real `stop_market` order
(`f"{o.order_type}" == '3'`). Fix: use the canonical `order_type_to_str(order.order_type)`
(yields `'STOP_MARKET'`) or `order.order_type.name`. Note this is ASCII either way,
so the non-Latin lint hook is unaffected; the issue is readability/actionability,
not encoding.

### Minor

**M-1. The rejection test cannot detect the I-2 readability regression.**
`[tests/integration_tests/adapters/sinopac/test_execution.py:2052-2056]`
`test_naked_conditional_order_is_rejected_with_emulation_hint` asserts only
`"emulation_trigger" in reason`. Because that substring is a static literal, the
test passes whether the order type renders as `STOP_MARKET` or as `(3)`. Asking
"what wrong implementation still passes?" — an implementation that emits an
unreadable order-type token does. Recommend additionally asserting the type name
appears (e.g. `assert order_type_to_str(order_type) in reason`), which both
strengthens the test and forces the I-2 fix.

**M-2. Task 2 test under-models the released-order round trip and overlaps an
existing test.** `[tests/integration_tests/adapters/sinopac/test_execution.py:2059-2088]`
`test_market_order_preserves_margin_tag_through_submit` relies on the bare
`AsyncMock()` from the `exec_client` fixture (no `return_value`), so the post-
`place_order` success path runs against a default `MagicMock` response rather than a
realistic `{"trade_id": ..., "status": "PendingSubmit"}` dict — unlike the sibling
tests (e.g. line 1676). This is also the most likely source of the
GC-attributed `RuntimeWarning: coroutine ... was never awaited` that surfaces at
this test's line in full-suite runs (see Verification). Setting an explicit
`return_value` would model the release path faithfully and remove the misleading
warning. Separately, this test substantially overlaps the existing
`test_margin_trading_passes_order_cond_enum` (line 1672); the added value is the
distinct `MARKET` + `IOC` path (vs the existing `LIMIT`), which is legitimate but
marginal. The plan/commit framing ("emulation-released") slightly overstates scope:
the test submits a `MARKET` order directly and does not exercise the `OrderEmulator`.

---

## Checklist results

1. **Correctness** — PASS. All 6 conditional types (`STOP_MARKET`, `STOP_LIMIT`,
   `MARKET_IF_TOUCHED`, `LIMIT_IF_TOUCHED`, `TRAILING_STOP_MARKET`,
   `TRAILING_STOP_LIMIT`) are excluded from `_NT_TO_SINOPAC_PRICE_TYPE`
   (`{LIMIT, MARKET, MARKET_TO_LIMIT}`) and all are members of
   `_CONDITIONAL_ORDER_TYPES`, so each reaches `generate_order_rejected`. The old
   silent `log + return` is gone.
2. **Error handling** — PARTIAL (see I-2). A real `OrderRejected` event is emitted
   (no silent drop); `reason` names `emulation_trigger` with concrete values
   (`LAST_PRICE or BID_ASK`). Degraded only by the opaque `(3)` order-type token.
3. **Security** — PASS. No injection into gateway calls; the rejection path makes no
   gateway call at all.
4. **API conformance** — PASS. `generate_order_rejected(strategy_id, instrument_id,
   client_order_id, reason, ts_event)` matches the Cython signature in
   `execution/client.pyx`; `self._clock.timestamp_ns()` matches the established
   `_submit_order` pattern.
5. **YAGNI** — PASS. No adapter-side trigger engine; emulation is delegated to
   NT-core `OrderEmulator`. The change is a minimal branch swap.
6. **Tests present** — PASS. 6 parametrized rejection cases + 1 tag test.
7. **Test acceptance logic** — PARTIAL (see M-1). The rejection test does assert
   `place_order.assert_not_called()` (real, can fail) and the loop is genuinely
   awaited via `run_until_complete`, so the early-return-before-`place_order`
   contract is enforced. The Task 2 test asserts `place_order.assert_awaited_once()`
   and `order_cond == SinopacOrderCond.MARGIN_TRADING` (real, can fail). Weakness is
   only the missing order-type-name assertion (M-1).
8. **Generated artifacts / doc sync** — N/A for this batch (doc audit is Task 4).
9. **Coverage domain vs accepted domain** — PASS. The parametrize list covers all 6
   conditional types — no subset. The defensive `else` branch is genuinely
   unreachable today (all 9 `OrderType` members are partitioned between the
   price-type map and the conditional set), so it correctly has no test.
10. **Quantitative claims** — One false claim found (I-2): the plan's assertion that
    the type renders as `OrderType.STOP_MARKET` is contradicted by code.
11. **ASCII discipline** — PASS. `.pre-commit-hooks/check_non_latin_text.sh` exits 0
    on both files; no non-ASCII in the new regions; `(3)` and the reason string are
    ASCII.

## State-machine verification (dispatch special attention)

- `(OrderStatus.INITIALIZED, OrderStatus.REJECTED)` is an explicitly valid
  transition (`nautilus_trader/model/orders/base.pyx:106`).
- The rejection `return`s immediately, so `generate_order_submitted`
  (`execution.py:979`) is never reached for conditional types — no double event, no
  `INITIALIZED -> SUBMITTED -> REJECTED` violation.
- The rejection test mocks `generate_order_rejected`, so the real transition is not
  exercised in-test, but it is the framework's own (tested upstream); acceptable for
  an adapter unit test.

## Verification commands

```
uv run --no-sync pytest tests/integration_tests/adapters/sinopac/ -q
# 114 passed, 1 warning in 0.48s

uv run --no-sync pytest .../test_execution.py -k "conditional or preserves_margin_tag" -W error::RuntimeWarning
# 7 passed (clean in isolation -> the full-suite RuntimeWarning is GC-attribution
#  from a pre-existing test, not introduced by this batch; see M-2)

uv run --no-sync ruff check tests/integration_tests/adapters/sinopac/test_execution.py
# I001 Import block is un-sorted or un-formatted  (-> I-1)

uv run --no-sync ruff check nautilus_trader/adapters/sinopac/execution.py
# All checks passed!

bash .pre-commit-hooks/check_non_latin_text.sh <both files>   # exit 0
```

## Required before approval

1. Fix I-1 (import order) so `ruff check` is clean on the test file.
2. Fix I-2 (use `order_type_to_str(order.order_type)` or `.name` in both reason
   branches) so the rejection reason names the order type.
3. Recommended: M-1 (assert the type name in the rejection test) and M-2 (set an
   explicit `place_order` `return_value` in the tag test).

---

## Fix Verification (2026-06-26, commit `b349a7564a` vs parent `98db183579`)

All four findings are resolved. Diff touches exactly the two files under review
(`execution.py` +9/-2, `test_execution.py` +7/-1) with no unrelated changes.

### Per-finding status

**I-1 — Import-ordering lint (ruff `I001`). RESOLVED.**
The fix moves `from nautilus_trader.common.factories import OrderFactory` out from
between the two `adapters.sinopac.execution` imports into the
`nautilus_trader.common.*` group (after `common.component`). A matching
`order_type_to_str` import was also sorted into the `model.enums` group in both
files.
`uv run --no-sync ruff check tests/integration_tests/adapters/sinopac/test_execution.py
nautilus_trader/adapters/sinopac/execution.py` -> `All checks passed!`

**I-2 — Rejection reason renders the order type by name. RESOLVED.**
Both reason branches in `_submit_order` (`execution.py`) now interpolate
`order_type_to_str(order.order_type)` instead of the bare enum:
- conditional branch: `"Sinopac has no native conditional orders ({order_type_to_str(order.order_type)}); resubmit with emulation_trigger=LAST_PRICE or BID_ASK to use NautilusTrader order emulation"`
- defensive else: `"Unsupported order type {order_type_to_str(order.order_type)} for Sinopac"`
Confirmed by direct evaluation that the f-string token rendered `'3'` before and
`order_type_to_str` renders `'STOP_MARKET'` (and the analogous names for all six
conditional types). The `emulation_trigger` guidance is retained, so the reason is
both human-readable and actionable. Output stays ASCII.

**M-1 — Rejection test can now catch an I-2 regression. RESOLVED.**
`test_naked_conditional_order_is_rejected_with_emulation_hint` adds
`assert order_type_to_str(order_type) in reason`. This assertion is genuinely
falsifiable: `order_type_to_str(OrderType.STOP_MARKET) == 'STOP_MARKET'`, which is
NOT a substring of a reason that rendered the integer `(3)`. A regression to the
bare enum would therefore fail this assertion rather than pass silently. The
existing `place_order.assert_not_called()` guard is retained.

**M-2 — `place_order` AsyncMock now models the success path. RESOLVED.**
`test_market_order_preserves_margin_tag_through_submit` now sets
`place_order = AsyncMock(return_value={"trade_id": "T-MARGIN-MKT", "code": "2330",
"status": "PendingSubmit"})`, matching the dict shape used by sibling tests. The
previously GC-attributed `RuntimeWarning: coroutine ... was never awaited` is gone:
the targeted run under `-W error::RuntimeWarning` passes, and the full-suite run no
longer reports the trailing warning.

### Verification commands

```
uv run --no-sync ruff check tests/integration_tests/adapters/sinopac/test_execution.py \
    nautilus_trader/adapters/sinopac/execution.py
# All checks passed!                                              (-> I-1 fixed)

uv run --no-sync pytest tests/integration_tests/adapters/sinopac/ -q
# 114 passed in 0.48s    (no warning line; was "114 passed, 1 warning")  (-> M-2 fixed)

uv run --no-sync pytest \
    ".../test_execution.py::test_market_order_preserves_margin_tag_through_submit" \
    -W error::RuntimeWarning -q
# 1 passed                                                        (-> M-2 confirmed)

bash .pre-commit-hooks/check_non_latin_text.sh \
    nautilus_trader/adapters/sinopac/execution.py \
    tests/integration_tests/adapters/sinopac/test_execution.py
# exit 0                                                          (-> ASCII discipline OK)
```

### Updated verdict: APPROVED

Both Important findings and both addressed Minor findings are resolved; the full
Sinopac suite is green (114 passed, no warnings) and lint is clean. No new findings
introduced by the fix.
