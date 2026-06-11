# nautilus-sinopac

[NautilusTrader](http://nautilustrader.io) adapter for [SinoPac Securities](https://www.sinotrade.com.tw/) (永豐金證券), providing access to Taiwan's stock (TWSE/TPEX), futures, and options (TAIFEX) markets.

The `nautilus-sinopac` crate provides client bindings (HTTP & WebSocket), data
models and helper utilities that connect through a self-hosted **FastAPI gateway**
([shioaji-server](https://github.com/Martingale42/shioaji-server)) bridging the
[Shioaji](https://sinotrade.github.io/) Python SDK.

## Wire-unit contract

Stock order quantity is expressed in **shares** end-to-end (gateway →
`list_trades` → NautilusTrader reconciliation). The gateway converts shares to
Shioaji lots (÷1000) at its SDK boundary, so a common-lot order quantity must be
a multiple of 1000 shares. Odd-lot quantities (1–999 shares) pass through as
shares. Futures and options quantities are in contracts.

## Panic safety

This crate is a real-money trading adapter, so all gateway-fed numbers cross a
hard panic-safety boundary before entering the domain model:

- Prices and quantities are built through checked constructors (`try_price` /
  `try_qty` in `common/parse.rs`) so NaN, infinite, negative, out-of-range, or
  over-precision values return errors instead of panicking.
- Length-mismatched bid/ask and OHLCV arrays are rejected rather than indexed
  out of bounds.
- KBar OHLC cross-field invariants are enforced via `Bar::new_checked`.
- A non-finite instrument `unit` (lot size) is rejected.
- A single poisoned WebSocket frame is contained with `catch_unwind` and logged,
  without killing the WebSocket receive loop.

## Production hardening

- Complete TAIFEX tick-size and contract-multiplier schedules
  (`common/tick_size.rs`, `common/instrument.rs`): index/sector roots
  (TXF/MXF/T5F/XIF/ZEF/ZFF), price-tiered single-stock-futures ticks, and the
  ETF-futures grid. Schedule selection keys off `underlying_kind` (S/I/E/C) plus
  the underlying code; unknown roots fall back to a default with a warning;
  fractional option strikes (e.g. 67.5) are preserved.
- The WebSocket emits a `{"event":"reconnected"}` sentinel after resubscription
  so the execution client can recover in-gap events via reconciliation.

## Platform

[NautilusTrader](http://nautilustrader.io) is an open-source, high-performance, production-grade
algorithmic trading platform, providing quantitative traders with the ability to backtest
portfolios of automated trading strategies on historical data with an event-driven engine,
and also deploy those same strategies live, with no code changes.

NautilusTrader's design, architecture, and implementation philosophy prioritizes software correctness and safety at the
highest level, with the aim of supporting mission-critical, trading system backtesting and live deployment workloads.

## Feature flags

This crate provides feature flags to control source code inclusion during compilation:

- `python`: Enables Python bindings from [PyO3](https://pyo3.rs).
- `extension-module`: Builds as a Python extension module.

## Documentation

See the [integration guide](https://nautilustrader.io/docs/developer_guide/adapters) for adapter development details.

## License

The source code for NautilusTrader is available on GitHub under the [GNU Lesser General Public License v3.0](https://www.gnu.org/licenses/lgpl-3.0.en.html).

---

NautilusTrader™ is developed and maintained by Nautech Systems, a technology
company specializing in the development of high-performance trading systems.
For more information, visit <https://nautilustrader.io>.

Use of this software is subject to the [Disclaimer](https://nautilustrader.io/legal/disclaimer/).

<img src="https://github.com/nautechsystems/nautilus_trader/raw/develop/assets/nautilus-logo-white.png" alt="logo" width="300" height="auto"/>

© 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
