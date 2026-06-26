# Sinopac Stop Orders — Batch 2 Review (Tasks 3-4)

**Date:** 2026-06-26
**Reviewer:** Code Reviewer (orchestrator pipeline)
**Scope:** Tasks 3-4 of `docs/plans/2026-06-26-sinopac-stop-orders.md`
**Commits:**
- `452db0bb44` — Add emulated stop_market and bracket scenarios to Sinopac exec tester
  (`examples/live/sinopac/sinopac_exec_tester.py`)
- `aba8dd801c` — Audit Sinopac integration doc against adapter code
  (`docs/integrations/sinopac.md`)

## Verdict: APPROVED WITH NOTES

Not CHANGES REQUESTED. The Task 4 integration-doc audit is accurate against the code on
every claim checked. The single substantive finding is mutually-contradictory and partly
inaccurate explanatory prose about bracket emulation in the Task 3 example file — a
comment/docstring defect, not a runtime bug. Tests are green (114 passed). Recommend the
prose be corrected in a follow-up; it does not block merge.

---

## Verification results

| Check | Command | Result |
|---|---|---|
| Sinopac suite | `uv run --no-sync pytest tests/integration_tests/adapters/sinopac/ -q` | **114 passed in 0.89s** |
| Exec-tester import + scenarios | `SINOPAC_EXEC_SCENARIO=stop_market SINOPAC_EXEC_DRY_RUN=1 uv run --no-sync python -c "import examples.live.sinopac.sinopac_exec_tester as t; print(t.SCENARIOS)"` | Prints `('common', 'intraday_odd', 'mkp', 'futures_octype', 'stop_market', 'bracket')` — no import error |
| Commit cross-check | `git log --oneline -20` | Both batch commits present at HEAD; no missed commits |

---

## Findings

### Critical
None.

### Important

1. **Contradictory + inaccurate bracket-emulation prose in the exec tester**
   [`examples/live/sinopac/sinopac_exec_tester.py:55-56`] vs
   [`examples/live/sinopac/sinopac_exec_tester.py:328-329`]

   The module docstring says:
   > "The stop-loss and take-profit legs are emulated; the entry limit is placed directly."

   The inline comment in `on_quote_tick` says:
   > "The SL leg is emulated in-process by the OrderEmulator; the TP leg is a plain LIMIT
   > resting at the venue."

   These two statements **contradict each other** (docstring: TP emulated; inline comment:
   TP rests at the venue), and **both are factually wrong** about the actual flow.

   Verified against the engine:
   - `OrderFactory.bracket(...)` constructs all three legs — entry `LIMIT`
     (`common/factories.pyx:207`), TP `LIMIT` (`:322`), SL `STOP_MARKET` (`:468`) — each
     with `emulation_trigger=emulation_trigger`. With `emulation_trigger=LAST_PRICE` all
     three are routed to the `OrderEmulator`, so none is "placed directly" / "resting at the
     venue" at submit time.
   - `OrderEmulator._handle_submit_order_list` (`execution/emulator.pyx:453-457`) skips the
     OTO children at submit ("Process contingency order later once parent triggered"), so at
     submission only the **entry** is held in the emulator; the **SL and TP activate after
     the entry fills**. All three are emulated; none rests natively at the venue (Sinopac has
     no native conditional order type — the whole premise of this feature).

   Severity Important because the Task 4 mandate is doc/comment-to-code fidelity, and this
   is freshly-introduced, self-contradictory prose that misdescribes a core mechanism. It is
   not Critical: the bracket is *built* correctly and per the plan's exact spec
   (`emulation_trigger=TriggerType.LAST_PRICE`); only the explanatory text is wrong.

   Suggested correction (single source, drop the per-leg "rests at venue" claim), e.g.:
   > "All three bracket legs (entry LIMIT, SL STOP_MARKET, TP LIMIT) carry
   > `emulation_trigger=LAST_PRICE`, so all are emulated by the OrderEmulator — the entry is
   > held immediately, and the OTO-child SL/TP activate only after the entry fills. Sinopac
   > has no native conditional orders, so no leg rests natively at the venue."

### Minor

2. **`MARKET_TO_LIMIT` row note phrasing** [`docs/integrations/sinopac.md:185`]
   "Range-market (MKP); stock orders rejected locally." In the context of the
   `MARKET_TO_LIMIT` row this clearly means stock `MARKET_TO_LIMIT` orders, which matches
   the local reject at `execution.py:425-428`. Slightly readable-as-ambiguous ("stock
   orders" in isolation), but accurate. No change required; noting for polish only. The
   plan's own wording ("stock MKP rejected locally") is marginally clearer.

---

## Checklist coverage

### Task 3 (exec tester) — `452db0bb44`

- **Imports used / clean import.** `OrderType` (used at `:338` `entry_order_type=OrderType.LIMIT`)
  and `TriggerType` (used at `:339`, `:430`, `:431`) are both used. Module imports cleanly;
  smoke test prints the 6-scenario tuple. PASS.
- **`stop_market` builds an EMULATED stop.** `_build_order` `:419-432` returns
  `order_factory.stop_market(... emulation_trigger=TriggerType.LAST_PRICE)`. Trigger set a
  tick above the ask for a BUY stop (fires on a rise) — matches plan intent. PASS.
- **`bracket` handled BEFORE the single-order path.** `on_quote_tick` `:326-345` handles
  `SCENARIO_BRACKET` and `return`s before `order = self._build_order(quote)` at `:347`. No
  double-submit, no single-order path leakage. PASS.
- **First-quote gate preserved.** `:322-324` (`if self._submitted: return; self._submitted =
  True`) runs before both the bracket and single-order paths. PASS.
- **Bracket uses `submit_order_list`, not `submit_order`.** `:344`. PASS.
- **Scenario wiring.** `SCENARIO_STOP_MARKET` and `SCENARIO_BRACKET` are not in the futures
  set, so `_build_scenario_config` (`:479-490`) routes both to the fixed-id stock path
  (`STOCK_INSTRUMENT_ID` 2330) with `external_order_claims` — matches "both trade the equity".
  Quantities are 2000 shares (= 2 common lots, multiple of 1000). PASS.
- **Dry-run handling.** Bracket path guards `submit_order_list` with `if not dry_run`;
  stop_market flows through the shared single-order dry-run skip. PASS.
- **Pyright noise** at `:222` (`frozen=True`) and the `make_price`/`make_qty is not a known
  attribute of None` hints in the new bracket code were triaged out of scope by the
  orchestrator and represent type-checker limitations (instrument is set in `on_start`
  before any quote), not runtime bugs. Not re-flagged.
- **Tests.** Task 3 is a manual integration probe; the plan's acceptance is the import/scenario
  smoke test (passes). No unit test expected. The full suite remains green.

### Task 4 (doc audit) — `aba8dd801c`

Every edited claim cross-checked against code; all accurate:

- **(a) Order-types matrix** [`sinopac.md:181-185`]. `MARKET`/`LIMIT`/`MARKET_TO_LIMIT`
  match `_NT_TO_SINOPAC_PRICE_TYPE` (`execution.py:170-174`). "MARKET coerced to IOC if TIF
  DAY/GTC" matches `_resolve_order_type` (`:230-235`). `MARKET_TO_LIMIT` Stocks ✗ / Futures
  ✓ / Options ✓ + "stock orders rejected locally" matches the stock-MKP local reject
  (`:425-428`). PASS.
- **(b) Emulated-stop subsection** [`:192-208`]. Lists `STOP_MARKET`, `STOP_LIMIT`,
  `MARKET_IF_TOUCHED`, `LIMIT_IF_TOUCHED`, and the trailing variants = exactly the 6 members
  of `_CONDITIONAL_ORDER_TYPES` (`execution.py:189-198`). Correctly states naked conditionals
  (no `emulation_trigger`) are rejected locally with a message pointing at `emulation_trigger`
  — matches the Batch 1 reject path. PASS.
- **(c) Odd-lot note** [`:225-230`]. "IntradayOdd supported via `SinopacOrderTags(order_lot=
  "IntradayOdd")`, shares 1-999, LIMIT + DAY; Odd/Fixing out of scope (B3)" matches
  `_validate...` (`execution.py:362-368`, IntradayOdd ⇒ LMT+ROD+1..999+Cash; DAY maps to ROD)
  and `_ORDER_LOT_BY_NAME` (`:250-253`) and `tags.py:39-42`. PASS.
- **(d) `order_lot` always sent** [`:212`]. "always sends an `order_lot` field (defaulting to
  `Common`)" matches `place_order(... order_lot=validated.order_lot ...)` (`execution.py:1041`).
  The previous stale "`_submit_order` does not send an `order_lot` field" claim is removed. PASS.
- **(e) Taiwan-tags matrix** [`:246-251`]. `order_lot` {Common, IntradayOdd},
  `order_cond` {Cash, MarginTrading, ShortSelling}, `daytrade_short` (requires Cash),
  `octype` {Auto, New, Cover, DayTrade} (Futures/Options) match `_ORDER_LOT_BY_NAME`,
  `_ORDER_COND_BY_NAME`, `_OCTYPE_BY_NAME` (`execution.py:250-264`), the `daytrade_short`
  rule (`:372-373`), and `tags.py:39-58`. PASS.
- **Single-source-of-truth linking** [`:242-244`, `:268-272`] points the integration doc at
  `nautilus_trader/adapters/sinopac/__init__.py` as canonical — consistent with that
  docstring's capability matrix. PASS.

### Cross-cutting

- **All 6 conditional types covered** (not a subset) in both the code set and the doc's stop
  subsection. PASS.
- **Quantitative claims** (1000-multiple common lot; 1-999 odd-lot shares) backed by
  `execution.py:365-370`. PASS.
- **ASCII discipline.** No rejection/log strings added in this batch (those were Batch 1).
  New exec-tester log strings ("Built bracket: ...") are ASCII. The Chinese glyphs in the doc
  (盤中零股 etc.) are permitted in Markdown prose. PASS.
- **YAGNI / security.** No adapter-side trigger engine; emulation stays NT-core. No injection
  surface into gateway calls introduced. PASS.

---

## Summary

- Critical: 0
- Important: 1 (contradictory/inaccurate bracket-emulation prose in the exec tester example)
- Minor: 1 (order-types row phrasing, polish only)

Task 4 (the high-risk doc audit) is clean. Task 3 functions correctly and matches the plan;
its only defect is explanatory text. APPROVED WITH NOTES.
