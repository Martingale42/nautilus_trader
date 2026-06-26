# Final Audit — Sinopac Emulated Stop Orders + Capability-Doc Audit

**Date:** 2026-06-26
**Scope:** Full feature-branch diff vs `sinopac-adapter-clean` (merge-base `034c70e788`).
**Method:** `/code-review` skill, effort `high` — 8 finder angles (line-by-line, removed-behavior,
cross-file tracer, reuse/simplification/efficiency, altitude, conventions, test-acceptance logic,
doc-vs-code accuracy) → 1-vote recall-biased verification.
**Suite at audit time:** `uv run --no-sync pytest tests/integration_tests/adapters/sinopac/ -q` → 114 passed.

## Verdict: PASS (no Critical findings)

The core feature is correct and well-verified:

- **Rejection path** (`execution.py::_submit_order`): all 6 `_CONDITIONAL_ORDER_TYPES` members reach
  `generate_order_rejected` (kwargs match the Cython base signature at `execution/client.pyx`), the
  early `return` precedes both `generate_order_submitted` and `place_order`, `order_type_to_str()`
  is correctly imported from `nautilus_trader.model.enums` and renders readable ASCII names, and
  `INITIALIZED → REJECTED` is a valid FSM transition.
- **Emulation boundary**: the `OrderEmulator` transforms emulated conditionals to `MarketOrder`/
  `LimitOrder` *before* `SubmitOrder` reaches the exec client, so the rejection can only fire for
  **naked** (non-emulated) conditionals — it cannot break legitimate emulation.
- **Exec tester**: `stop_market` builds an emulated stop; `bracket` is handled before the single-order
  path and submits via `submit_order_list`; the first-quote gate is preserved.
- **Doc audit**: every *stated* claim in `docs/integrations/sinopac.md` matches code (order-types
  matrix, the 6-member conditional list, the Taiwan-tags matrix, "always sends `order_lot`").

### Fixed during audit (Critical-of-scope: Task 4 doc completeness)

- **Doc completeness — IntradayOdd `Cash`-only constraint** (`docs/integrations/sinopac.md`
  odd-lot note). `_validate_stock_lot_rules` (`execution.py:362-368`) enforces **three** rules for
  `IntradayOdd` — `LMT + ROD`, 1–999 shares, **and `order_cond == Cash`** — but the note listed only
  the first two. A user attaching `SinopacOrderTags(order_lot="IntradayOdd", order_cond="MarginTrading")`
  satisfied every documented rule yet is rejected locally. This is the exact doc-vs-code drift Task 4
  set out to eliminate (a *completeness* gap the per-batch review's *soundness* check did not surface).
  **Fixed inline**: the note now states the `Cash` requirement explicitly.

## BACKLOG (non-blocking — none block merge)

All findings below are Low/Medium and confined to example tooling, test robustness, or by-design
observations. None affects production correctness of the shipped feature.

### Example tester (`examples/live/sinopac/sinopac_exec_tester.py`)

- **B-1 — bracket TP leg inherits `tp_post_only=True`.** `order_factory.bracket(...)` is called without
  `tp_post_only=False`, so the take-profit LIMIT leg carries `post_only=True`. The Sinopac adapter has no
  `post_only` handling and the node config sets `use_post_only=False` ("not applicable to Taiwan
  exchange"), so the intent is silently dropped. Harmless today (adapter ignores it) but contradicts the
  venue's stated posture. *Fix:* pass `tp_post_only=False` in the bracket call. (Low)
- **B-2 — bracket branch bypasses the `None`-instrument guard.** The single-order `_build_order` path
  guards `if self.instrument is None: log.error("No instrument loaded"); return None`; the inserted
  bracket branch accesses `self.instrument.make_price(...)` directly. Largely unreachable (`on_start`
  stops the strategy when the instrument can't resolve) but divergent from the sibling path. (Low)
- **B-3 — bracket branch omits the dry-run warning.** Other scenarios emit a `WARNING` confirming the
  submit was intentionally skipped in dry-run; the bracket branch only logs `info("Built bracket: ...")`,
  so an operator scanning for the dry-run marker sees none. (Low)
- **B-4 — emulated bracket entry may not fire in sim.** The entry is a BUY LIMIT priced at `quote.ask`
  carrying `emulation_trigger=LAST_PRICE`, so the emulator holds it; depending on LIMIT-release
  semantics under `LAST_PRICE`, the marketable-by-intent entry can sit unreleased and the OTO SL/TP never
  arm, producing no terminal observable in a sim run. Usability nuance of a manual test tool. (Low)
- **B-5 (altitude, by-design) — asymmetric scenario layering.** `stop_market` is built inside
  `_build_order` (single Order) while `bracket` is special-cased atop `on_quote_tick` (OrderList). The
  asymmetry is justified — a bracket is an `OrderList`, not a single order — but means submit/dry-run
  handling lives in two places. Acceptable as-is; noted for future refactor. (Low / WONTFIX)

### Tests (`tests/integration_tests/adapters/sinopac/test_execution.py`)

- **T-1 — rejection test does not assert the real terminal state.** `generate_order_rejected` is mocked,
  so the test never asserts the order actually reaches `REJECTED`, nor that the correct `client_order_id`
  is passed. A mis-routed rejection event (wrong/stale id leaving the order in `INITIALIZED` limbo) would
  pass, since the test only substring-checks `reason`. *Fix:* also assert
  `call_args.kwargs["client_order_id"] == order.client_order_id`. (Low)
- **T-2 — rejection test does not assert `generate_order_submitted` was NOT called.** A regression
  emitting a spurious `SUBMITTED` before the reject would not be caught (the test only checks
  `place_order` not-called + reject called once). (Low)
- **T-3 — margin-tag test pins only `order_cond`.** It asserts `kwargs["order_cond"] == MARGIN_TRADING`
  but not `action`/`code`/`octype`/`order_lot`; a wrong-side (SELL→BUY) or wrong-octype margin order on
  the released path would pass. (Low)
- **T-4 — duplication: SubmitOrder boilerplate.** The two new tests inline `cache.add_order` +
  `SubmitOrder(...)` + `event_loop.run_until_complete(...)` instead of the existing
  `_submit_built_order` async helper, introducing a second async-invocation idiom. (Low / cleanup)
- **T-5 — overlap with existing margin test.** `test_market_order_preserves_margin_tag_through_submit`
  overlaps `test_margin_trading_passes_order_cond_enum`; only the MARKET/SELL/IOC path is genuinely new.
  (Low / cleanup — kept intentionally per Batch 1 review)
- **T-6 — section header overstates scope.** The banner "Tag preservation on emulation-released orders"
  labels a test that submits a plain MARKET order directly; the `OrderEmulator` is never exercised.
  *Fix:* reword the header to "...on released (MARKET) orders" to avoid implying untested emulator
  coverage. (Low / comment accuracy)

### Adapter (`nautilus_trader/adapters/sinopac/execution.py`)

- **A-1 (by-design) — `_CONDITIONAL_ORDER_TYPES` is the complement of the supported map.** The 6 listed
  members are exactly `OrderType` minus the 3 in `_NT_TO_SINOPAC_PRICE_TYPE`, and the defensive `else`
  branch is currently unreachable. This is intentional (the plan added it as a guard against future enum
  additions and documents it inline). Noted as a design observation, not a defect. (WONTFIX / by-design)
- **A-2 — dropped `"Unsupported order type:"` ERROR log literal.** The old silent branch logged that
  string; the new path emits an `OrderRejected` event instead (strictly better). Any log-scraping monitor
  keyed on the old literal loses the signal. Speculative; the event-based path is the correct design.
  (Low / informational)

---

*No Critical findings → audit passes with 0 fix cycles. The single in-scope doc-completeness gap was
fixed inline. All other items are tracked above for future hardening of the example tooling and tests.*
