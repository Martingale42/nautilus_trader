# Batch 1 Review (Tasks 1-4)

**Date:** 2026-03-20
**Reviewer:** Claude (Code Reviewer)
**Commits:** d7163337b, 1baebf4f0, 22a512e7e, eb4662347
**Plan:** `docs/plans/2026-03-20-sinopac-structural-refactor.md`

## Verdict: APPROVED WITH NOTES

All four tasks are correctly implemented. Wire formats are preserved, tests pass, and the code follows NT standards. Two minor items noted below.

---

## Verification Results

| Check | Result |
|-------|--------|
| `cargo clippy -p nautilus-sinopac --all-targets` | Clean (no warnings) |
| `cargo +nightly fmt -p nautilus-sinopac -- --check` | Clean |
| `cargo test -p nautilus-sinopac` | 71/71 passed (63 unit + 5 HTTP + 3 WS) |
| No `tracing::` usage | Clean |
| No bare `#[test]` (all `#[rstest]`) | Clean |
| No `Optional[]`/`Union[]` in Python (PEP 604) | Clean |

---

## Task-by-Task Review

### Task 1: Clean up `_trade_id_to_client_order_id` (d7163337b)

**Files:** `nautilus_trader/adapters/sinopac/execution.py`

- Correctly adds `.pop(order_id, None)` after `generate_order_rejected` (line 252)
- Correctly adds `.pop(order_id, None)` after `generate_order_canceled` (line 282)
- Correctly adds conditional `.pop(trade_id_str, None)` after `generate_order_filled` only when `order.is_closed` (lines 352-355)
- Correctly does NOT add cleanup at `_submit_order` rejection (entry never added on that path)
- Uses safe `.pop(key, None)` to avoid KeyError on missing entries

**No findings.**

### Task 2: Type WsCommand with SinopacQuoteType (1baebf4f0)

**Files:** `websocket/mod.rs`, `websocket/messages.rs`, `websocket/client.rs`, `tests/websocket.rs`

- `WsCommand` variants correctly changed from `String` to `SinopacQuoteType`
- `WsSubscribeMsg.quote_type` correctly changed from `String` to `SinopacQuoteType`
- `subscribe()`/`unsubscribe()` signatures updated to accept `SinopacQuoteType`
- `handler.rs` works without changes because it destructures by field name and the type change is transparent
- Wire format preserved: `SinopacQuoteType` has `#[serde(rename_all = "lowercase")]`, so `Tick` -> `"tick"`, `BidAsk` -> `"bidask"`
- Tests updated to use `SinopacQuoteType::Tick` and `SinopacQuoteType::BidAsk`
- Doc comments added to `WsCommand` variants (good improvement)

**No findings.**

### Task 3: Type PlaceOrderRequest with enums (22a512e7e)

**Files:** `http/models.rs`, `python/http.rs`

- All six string fields in `PlaceOrderRequest` correctly typed with their respective enums
- `py_place_order` signature updated to accept enum types
- Default values in `#[pyo3(signature = (...))]` correctly use enum variants (e.g., `SinopacPriceType::LMT`)
- Wire format preserved for all enums:
  - `SinopacAction`: no rename_all, `Buy`/`Sell` serialize as `"Buy"`/`"Sell"` (matches old strings)
  - `SinopacPriceType`: no rename_all, `LMT`/`MKT` serialize as `"LMT"`/`"MKT"` (matches)
  - `SinopacOrderType`: no rename_all, `ROD`/`IOC`/`FOK` (matches)
  - `SinopacOrderCond`: no rename_all, `Cash`/`MarginTrading`/`ShortSelling` (matches)
  - `SinopacOrderLot`: no rename_all, `Common`/`Odd`/`IntradayOdd`/`Fixing` (matches)
  - `SinopacMarket`: `rename_all = "lowercase"`, `Stock`->`"stock"`, etc. (matches)

**No findings.**

### Task 4: Type Python subscribe calls and execution mappings (eb4662347)

**Files:** `python/websocket.rs`, `data.py`, `execution.py`

- `py_subscribe`/`py_unsubscribe` correctly changed from `String` to `SinopacQuoteType`
- `data.py` subscribe/unsubscribe calls updated to use `SinopacQuoteType.TICK` and `SinopacQuoteType.BID_ASK`
- Mapping dicts (`_NT_TO_SINOPAC_ACTION`, `_NT_TO_SINOPAC_PRICE_TYPE`, `_NT_TO_SINOPAC_ORDER_TYPE`) correctly use enum values
- `_determine_market` return type correctly annotated as `-> SinopacMarket`
- `_determine_market` docstring uses imperative mood ("Determine")
- `_submit_order` correctly passes enum values to `place_order()`

**No findings.**

---

## Findings

### Minor

1. **[execution.py:652] `generate_position_status_reports` still uses string literals for market**
   The loop `for market in ("stock", "futures")` still uses raw strings instead of `SinopacMarket.STOCK`/`SinopacMarket.FUTURES`. This is acceptable because the Rust `py_list_positions` method still takes `&str` for the `market` parameter. The plan acknowledged this boundary: "the `list_positions` Rust method currently takes `market: &str`, so the Python side may need to call `.value` or the Rust side needs updating." This can be addressed in a future task when `py_list_positions` is also typed.

2. **[execution.py:16-19] Imports could be consolidated into a single `from` statement**
   The four separate `from nautilus_trader.core.nautilus_pyo3.sinopac import X` lines could be a single multi-import. This is a style-only observation and consistent with how the file's other imports are structured (one per line), so no change needed.

---

## Summary

All four batch 1 tasks are correctly implemented with no correctness, security, or API conformance issues. Wire formats are verified preserved for all serde-serialized enums. All verification checks pass. The two minor notes do not block approval.
