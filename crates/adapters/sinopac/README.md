# nautilus-sinopac

[NautilusTrader](http://nautilustrader.io) adapter for [SinoPac Securities](https://www.sinotrade.com.tw/) (永豐金證券), providing access to Taiwan's stock (TWSE/TPEX), futures, and options (TAIFEX) markets.

The `nautilus-sinopac` crate provides client bindings (HTTP & WebSocket), data
models and helper utilities that connect through a self-hosted **FastAPI gateway**
([shioaji-server](https://github.com/Martingale42/shioaji-server)) bridging the
[Shioaji](https://sinotrade.github.io/) Python SDK.

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
