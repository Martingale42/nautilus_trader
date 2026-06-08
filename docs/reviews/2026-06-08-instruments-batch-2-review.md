# Batch 2 (WS-B) Code Review — Sinopac Adapter Rust Instrument Parse Fixes

- **Date:** 2026-06-09
- **Repo / Branch:** `nautilus_trader` @ `sinopac-adapter-clean`
- **Reviewer:** Code Reviewer (instrument-definition pipeline)
- **Scope:** WS-B — sinopac adapter Rust instrument parse fixes
- **Commits reviewed:**
  - `016b17b3` — `http/models.rs`: add `unit: f64` / `multiplier: i64` / `currency: String` (+ `underlying_code: String` on futures/options), all `#[serde(default)]`.
  - `ed739d9e` — `http/parse.rs` (authoritative multiplier/unit/currency + `option_right` "C"/"P" fix + `parse_currency_or_twd`), `common/instrument.rs` (removed dead `extract_root_symbol`, kept tables as fallback), `models.rs`/`tests/http.rs` assertions, and the 3 `test_data/contracts_*.json` fixtures.
- **Plan / design:**
  - `shioaji-server/docs/plans/2026-06-08-ws-b-adapter-instrument-parse.md`
  - `shioaji-server/docs/plans/2026-06-08-instrument-definitions-design.md` (WS-B)

## Verdict: APPROVED WITH NOTES

The code is correct, matches the WS-B plan precisely, has no `unwrap()`/`expect()`/panic path on external gateway data, and is covered by meaningful financial-correctness tests. All 85 cargo tests pass. The single "NOTES" item is an **environment** finding (the canonical `make build-debug` / `uv run` path is blocked by a uv version pin) — it does not affect the correctness of the Rust code and does not block this batch, because the cargo gate is green.

---

## Findings

### Critical
None.

### Important
None in the code.

**I1 (Environment, not code) — canonical `make build-debug` / `uv run` path is version-gated and currently broken in this environment.**
`pyproject.toml:120` pins `required-version = "==0.11.6"` but the environment's global uv is `0.8.22`, so every `uv run`/`make` invocation fails the version gate:
```
error: Required uv version `==0.11.6` does not match the running version `0.8.22`.
```
Consequence: the Python integration suite (`tests/integration_tests/adapters/sinopac/`) cannot be run through the canonical path without a scoped uv 0.11.6, and none is cached locally. This is a team-infra problem (canonical build/test path is unusable as documented), not a defect in the WS-B diff. Recommend the team either relax the pin or provision a scoped 0.11.6 so the documented `make build-debug` + `uv run --active --no-sync pytest …` flow works again. Per the review brief I did NOT mutate global uv or re-sync the venv.

### Minor
**M1 — `parse_currency_or_twd` silently coerces an unrecognized non-empty currency code to TWD** (`http/parse.rs:349-360`). This is the intended/documented behavior (the docstring states TWD is a safe default for all Taiwan venue instruments, and a missing field must never panic), and it is the right call for a no-panic external-data parser. Noting only that an unrecognized non-empty code is swallowed without a log; for a Taiwan-only venue this is acceptable and YAGNI-correct, so no change required.

**M2 — `activation_ns` uses `parse_date_to_nanos(&contract.update_date).unwrap_or(ts_event)`** (`http/parse.rs`, futures path, pre-existing — not introduced by this batch). It is a no-panic `unwrap_or`, so it does not violate the no-`unwrap()` rule. Listed only for completeness; out of scope for WS-B.

---

## Batch-2-specific checks (all PASS)

| Check | Result | Evidence |
|---|---|---|
| `option_right` "C"→Call / "P"→Put | PASS | `parse.rs` match `"C"=>Call, "P"=>Put`; tests `test_option_right_c_parses_call`, `test_option_right_p_parses_put` |
| Unknown `option_right` still `bail!`s (no silent default) | PASS | `other => anyhow::bail!("Unknown option_right {other:?} (expected 'C'/'P')")`; test `test_option_right_unknown_bails` (uses the legacy spelling "Call" → now errors) |
| Authoritative `multiplier` used; table only when `multiplier == 0` | PASS | `if contract.multiplier > 0 { … as f64 } else { futures_multiplier/options_multiplier(…) }`; tests `test_futures_uses_authoritative_multiplier` (777, not in table), `test_options_uses_authoritative_multiplier` (99 ≠ TXO 50), and the two `*_zero_falls_back_to_table` tests (→200 / →50) |
| `unit` flows to `lot_size` (fallback STOCK/CONTRACT lot) | PASS | `if contract.unit > 0.0 { unit } else { STOCK_LOT_SIZE / CONTRACT_LOT_SIZE }`; tests `test_stock_unit_sets_lot_size` (100), `test_futures_unit_sets_lot_size` (5), `test_stock_missing_unit_falls_back_to_default_lot` (1000) |
| `underlying` uses `underlying_code` (fallback root symbol) | PASS | `if contract.underlying_code.is_empty() { Ustr::from(root_symbol) } else { Ustr::from(underlying_code) }` in both futures & options |
| `currency` authoritative, fallback TWD, no panic | PASS | `parse_currency_or_twd` → `Currency::try_from_str(code).unwrap_or_else(Currency::TWD)`; `try_from_str` returns `Option`, so no panic on bad code; empty → TWD early-return |
| `#[serde(default)]` on all new fields | PASS | all 11 new fields carry `#[serde(default)]` (commit `016b17b3`) so partial gateway responses still deserialize |
| Fixtures assert new gateway shape | PASS | `contracts_options.json` `"option_right": "C"`, `multiplier: 50`, `underlying_code: "TXO"`; `contracts_futures.json` `multiplier: 200`, `underlying_code: "TXF"`; `contracts_stocks.json` `unit: 1000.0`; `tests/http.rs` asserts `option_right == "C"`, `multiplier == 50`, `underlying_code == "TXO"` |
| Dead code removed | PASS | `extract_root_symbol` + its 5 tests deleted from `common/instrument.rs`; hardcoded `futures_multiplier`/`options_multiplier` retained as documented fallback |
| No `unwrap()`/`expect()` on external data in production paths | PASS | grep of added lines: all `unwrap()` are inside `#[cfg(test)]` test bodies; no `expect()` added |

## Consumes Batch-1 (WS-A) gateway shape correctly
WS-A now emits `option_right` as `.value` ("C"/"P") and adds `currency` ("TWD") + `unit`/`multiplier`/`underlying_code`. The Rust parser consumes exactly that shape: `option_right` matches "C"/"P" (legacy "Call"/"Put" now correctly `bail!`s), currency parses the ISO code string, and the numeric fields are read directly. Shapes are aligned.

---

## Verification output

### `cargo check -p nautilus-sinopac` — clean compile
```
warning: `nautilus-sinopac` (lib) generated 3 warnings
    Finished `dev` profile [unoptimized] target(s) in 9.58s
```
The 3 warnings are pre-existing `dead_code` on `BIDASK_EMIT_*` constants in `websocket/client.rs` — unrelated to this batch.

### `cargo test -p nautilus-sinopac --features python` — primary WS-B gate, ALL PASS
```
running 77 tests
test result: ok. 77 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 5 tests   (tests/http.rs)
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 3 tests   (tests/websocket.rs)
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 0 tests   (Doc-tests)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```
Total **85 tests passed, 0 failed**. The new WS-B tests all pass:
`test_option_right_c_parses_call`, `test_option_right_p_parses_put`, `test_option_right_unknown_bails`,
`test_futures_uses_authoritative_multiplier`, `test_futures_multiplier_zero_falls_back_to_table`,
`test_options_uses_authoritative_multiplier`, `test_options_multiplier_zero_falls_back_to_table`,
`test_futures_unit_sets_lot_size`, `test_stock_unit_sets_lot_size`, `test_stock_missing_unit_falls_back_to_default_lot`.

### Python integration tests (`tests/integration_tests/adapters/sinopac/`) — DEFERRED (env-blocked)
The canonical path is version-gated and could not be run:
```
$ uv --version
uv 0.8.22
$ grep required-version pyproject.toml
required-version = "==0.11.6"
$ uv run --active --no-sync pytest tests/integration_tests/adapters/sinopac/ -q
error: Required uv version `==0.11.6` does not match the running version `0.8.22`.
        Update `uv` by running `uv self update 0.11.6`.
```
No scoped uv 0.11.6 is cached on this machine (`~/.local/share/uv` has no matching binary). Per the review brief, global uv was NOT mutated and the venv was NOT re-synced. The pyo3 `.so` artifacts exist in the tree, but pytest can only be launched through the version-gated `uv`, so the integration run is deferred and treated as best-effort. The verdict therefore rests on the cargo test gate (85/85 green) + code review, which is the documented fallback. See finding **I1** for the infra fix.

### Lock-file hygiene
- Neither commit touches `Cargo.lock`/`uv.lock` (`git show --stat 016b17b3 ed739d9e` shows no lock files).
- The working tree's `M Cargo.lock` / `M uv.lock` churn is pre-existing and unrelated to this batch; it was NOT staged.

---

## Summary

WS-B faithfully implements the plan: gateway contract structs gained `unit`/`multiplier`/`currency`/`underlying_code` (all `#[serde(default)]`), and the three `parse_*` functions now prefer Shioaji's authoritative `multiplier`/`unit`/`currency`/`underlying_code`, with the hardcoded `futures_multiplier`/`options_multiplier` tables demoted to a `multiplier == 0` fallback. The `option_right` bug is fixed and aligned to WS-A's "C"/"P" — options no longer wholesale `bail!`, and an unknown value still `bail!`s with the offending value attached. There is no `unwrap()`/`expect()`/panic on external data: `parse_currency_or_twd` uses `try_from_str(...).unwrap_or_else(Currency::TWD)`, and every numeric/underlying fallback is a plain conditional. Tests are meaningful (non-table multipliers 777/99 prove the authoritative path; zero proves fallback; non-default units prove lot-size flow; the legacy "Call" spelling proves the bail). The only open item is environmental: the canonical `make build-debug` / `uv run` path is broken by a `required-version == 0.11.6` pin against a global uv 0.8.22, which deferred the Python integration suite — flagged for the team to fix.
