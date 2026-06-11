#!/usr/bin/env python3
"""
Example: Sinopac execution client tester.

Tests order submission, modification, and cancellation for Taiwan instruments
using the Sinopac gateway adapter.

Prerequisites:
    1. Sinopac gateway running: ``uvicorn sinopac_server.main:app --port 8000``
    2. Gateway logged in with CA activated for order placement
    3. Use simulation=True in gateway for testing

CAUTION: Set dry_run=True to prevent actual order placement.

Scenarios
---------
The ``SINOPAC_EXEC_SCENARIO`` environment variable selects which strategy runs:

- ``common`` (default): the stock ``ExecTester`` behavior (unchanged). Exercises
  the generic limit/cancel/replace path on 2330 (TSMC).
- ``intraday_odd``: submits a single 37-share intraday odd-lot LIMIT @ bid, ROD,
  tagged ``order_lot=IntradayOdd``. Verifies the Taiwan intraday odd-lot path
  (LMT+ROD, 1-999 shares, share-unit factor 1).
- ``mkp``: submits a single MARKET_TO_LIMIT (MKP, range-market) order. MKP is
  futures/options-only on the Shioaji side, so this scenario targets the MXF
  front-month futures contract (a stock MKP is rejected locally by the adapter,
  which would only exercise the local-reject path already covered by unit tests).
  The adapter coerces the default GTC TIF to IOC for marketable order types.
- ``futures_octype``: submits a single MXF front-month LIMIT tagged
  ``octype=Cover``. With no open position a Cover order is rejected by the venue;
  reaching that terminal rejection is the expected observable.

Margin/short-selling (``order_cond=MarginTrading``/``ShortSelling``) is NOT
scripted here: the simulation gateway has no credit account, so those paths
cannot reach a meaningful terminal state in sim. They are a manual pre-live
verification item (place a real margin/short order against a funded credit
account during a controlled live session) -- see the design doc test matrix.

The ``SINOPAC_EXEC_DRY_RUN`` environment variable (``true``/``false``) overrides
``dry_run`` for the scenario strategies; when true, orders are built and logged
but not submitted.
"""

import os
from decimal import Decimal

from nautilus_trader.adapters.sinopac.config import SinopacDataClientConfig
from nautilus_trader.adapters.sinopac.config import SinopacExecClientConfig
from nautilus_trader.adapters.sinopac.constants import SINOPAC
from nautilus_trader.adapters.sinopac.factories import SinopacLiveDataClientFactory
from nautilus_trader.adapters.sinopac.factories import SinopacLiveExecClientFactory
from nautilus_trader.adapters.sinopac.tags import SinopacOrderTags
from nautilus_trader.cache.config import CacheConfig
from nautilus_trader.common.enums import LogColor
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.config import LiveExecEngineConfig
from nautilus_trader.config import LoggingConfig
from nautilus_trader.config import StrategyConfig
from nautilus_trader.config import TradingNodeConfig
from nautilus_trader.live.node import TradingNode
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.enums import OrderSide
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.events import OrderEvent
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.identifiers import TraderId
from nautilus_trader.model.instruments import Instrument
from nautilus_trader.model.orders import Order
from nautilus_trader.test_kit.strategies.tester_exec import ExecTester
from nautilus_trader.test_kit.strategies.tester_exec import ExecTesterConfig
from nautilus_trader.trading.strategy import Strategy


# --- Shared configuration ------------------------------------------------------

# Quantities are SHARES end-to-end (gateway wire unit): 1000 shares = 1 common lot,
# which the gateway converts to 1 SDK lot at the boundary. Was Decimal(1) under the
# old lots-based wire unit; that now means 1 share (odd-lot) and a common-lot order
# of 1 share is rejected as a non-1000-multiple.
STOCK_INSTRUMENT_ID = InstrumentId.from_str("2330.SINOPAC")  # TSMC
# MXF (Mini-TAIEX) front-month futures, "C0" = current month per Shioaji naming.
FUTURES_INSTRUMENT_ID = InstrumentId.from_str("MXFC0.SINOPAC")

TRADE_SIZE = Decimal(1000)  # 1000 shares = 1 common lot
OFFSET_TICKS = 10  # Offset from market price for limit orders
SINOPAC_ACCOUNT_ID = None  # Set to your account ID, or use SINOPAC_ACCOUNT_ID env var
GATEWAY_HOST = "localhost"
GATEWAY_PORT = 8123  # gateway moved off the popular 8000 (collided with vLLM)

# Places REAL orders so the full path reaches shioaji-server (dry_run=True would
# short-circuit inside NT before the order is sent, never exercising the gateway
# end-to-end). The configured gateway (:8123) is the SIMULATION gateway -- never
# point this at a live gateway. The market-open cron wrapper additionally refuses
# to run this tester unless the gateway reports simulation=true.
DRY_RUN_DEFAULT = False

SCENARIO_COMMON = "common"
SCENARIO_INTRADAY_ODD = "intraday_odd"
SCENARIO_MKP = "mkp"
SCENARIO_FUTURES_OCTYPE = "futures_octype"
SCENARIOS = (
    SCENARIO_COMMON,
    SCENARIO_INTRADAY_ODD,
    SCENARIO_MKP,
    SCENARIO_FUTURES_OCTYPE,
)


def _env_dry_run() -> bool:
    """
    Resolve the dry-run flag from ``SINOPAC_EXEC_DRY_RUN``.

    Returns
    -------
    bool
        ``True`` if the env var is set to a truthy token, otherwise the module
        default (``DRY_RUN_DEFAULT``).

    """
    raw = os.environ.get("SINOPAC_EXEC_DRY_RUN")
    if raw is None:
        return DRY_RUN_DEFAULT
    return raw.strip().lower() in ("1", "true", "yes", "on")


def _resolve_scenario() -> str:
    """
    Resolve the active scenario from ``SINOPAC_EXEC_SCENARIO``.

    Returns
    -------
    str
        One of ``SCENARIOS``; defaults to ``common`` when unset.

    Raises
    ------
    ValueError
        If the env var holds an unknown scenario name.

    """
    scenario = os.environ.get("SINOPAC_EXEC_SCENARIO", SCENARIO_COMMON).strip().lower()
    if scenario not in SCENARIOS:
        raise ValueError(
            f"Unknown SINOPAC_EXEC_SCENARIO '{scenario}', expected one of {SCENARIOS}",
        )
    return scenario


# --- Order-semantics scenario strategy ----------------------------------------


class OrderSemanticsScenarioConfig(StrategyConfig, frozen=True):
    """
    Configuration for ``OrderSemanticsScenarioStrategy``.

    Parameters
    ----------
    instrument_id : InstrumentId
        The instrument to subscribe to and trade.
    scenario : str
        The scenario name, one of ``intraday_odd``, ``mkp``, ``futures_octype``.
    dry_run : bool, default False
        If true, the order is built and logged but not submitted.

    """

    instrument_id: InstrumentId
    scenario: str
    dry_run: bool = False


class OrderSemanticsScenarioStrategy(Strategy):
    """
    Submit exactly one Taiwan order-semantics order, then observe its lifecycle.

    On the first quote tick the strategy builds a single order tailored to the
    configured scenario, submits it (unless ``dry_run``), and logs every order
    event it receives. Any resting order is cancelled on stop. This is a manual
    integration probe with no alpha; it exists to exercise the full NT -> gateway
    -> Shioaji order path for the Taiwan-specific order semantics.

    """

    def __init__(self, config: OrderSemanticsScenarioConfig) -> None:
        super().__init__(config)
        self.instrument: Instrument | None = None
        self.order: Order | None = None
        self._submitted = False

    def on_start(self) -> None:
        """
        Subscribe to quotes for the configured instrument.
        """
        self.instrument = self.cache.instrument(self.config.instrument_id)
        if self.instrument is None:
            self.log.error(f"Could not find instrument for {self.config.instrument_id}")
            self.stop()
            return

        self.log.info(
            f"Scenario '{self.config.scenario}' armed on {self.config.instrument_id} "
            f"(dry_run={self.config.dry_run})",
            LogColor.BLUE,
        )
        self.subscribe_quote_ticks(self.config.instrument_id)

    def on_quote_tick(self, quote: QuoteTick) -> None:
        """
        Submit the single scenario order on the first quote received.
        """
        if self._submitted:
            return
        self._submitted = True  # Guard before building so a failure does not loop

        order = self._build_order(quote)
        if order is None:
            return

        self.order = order
        self.log.info(
            f"Built {self.config.scenario} order: {order!r} tags={order.tags}",
            LogColor.GREEN,
        )

        if self.config.dry_run:
            self.log.warning("Dry run, skipping submit")
            return

        self.submit_order(order)

    def _build_order(self, quote: QuoteTick) -> Order | None:
        """
        Build the single order for the active scenario.

        Parameters
        ----------
        quote : QuoteTick
            The latest quote, used to price the order at the bid.

        Returns
        -------
        Order or ``None``
            The scenario order, or ``None`` if the scenario name is unhandled.

        """
        instrument = self.instrument
        if instrument is None:
            self.log.error("No instrument loaded")
            return None

        if self.config.scenario == SCENARIO_INTRADAY_ODD:
            # 37-share intraday odd lot, LIMIT @ bid, ROD. The adapter validates
            # LMT+ROD+1..999 shares+Cash locally before the gateway.
            return self.order_factory.limit(
                instrument_id=self.config.instrument_id,
                order_side=OrderSide.BUY,
                quantity=instrument.make_qty(Decimal(37)),
                price=quote.bid_price,
                time_in_force=TimeInForce.DAY,  # mapped to ROD by the adapter
                tags=[SinopacOrderTags(order_lot="IntradayOdd").value],
            )

        if self.config.scenario == SCENARIO_MKP:
            # MARKET_TO_LIMIT -> Shioaji MKP (range market). MKP is futures/options
            # only, so this targets the MXF front-month future. The adapter coerces
            # the default GTC TIF to IOC for marketable order types.
            return self.order_factory.market_to_limit(
                instrument_id=self.config.instrument_id,
                order_side=OrderSide.BUY,
                quantity=instrument.make_qty(Decimal(1)),  # 1 futures contract
            )

        if self.config.scenario == SCENARIO_FUTURES_OCTYPE:
            # 1x MXF front-month LIMIT @ bid tagged octype=Cover. With no open
            # position a Cover order is rejected by the venue; that terminal
            # rejection is the expected observable.
            return self.order_factory.limit(
                instrument_id=self.config.instrument_id,
                order_side=OrderSide.SELL,
                quantity=instrument.make_qty(Decimal(1)),
                price=quote.bid_price,
                time_in_force=TimeInForce.DAY,
                tags=[SinopacOrderTags(octype="Cover").value],
            )

        self.log.error(f"Unhandled scenario '{self.config.scenario}'")
        return None

    def on_order_event(self, event: OrderEvent) -> None:
        """
        Log every order event so the terminal state is observable in the log.
        """
        self.log.info(f"ORDER EVENT: {event!r}", LogColor.MAGENTA)

    def on_stop(self) -> None:
        """
        Cancel any resting scenario order and unsubscribe.
        """
        if not self.config.dry_run:
            self.cancel_all_orders(self.config.instrument_id)
        self.unsubscribe_quote_ticks(self.config.instrument_id)


# --- Node assembly -------------------------------------------------------------


def _scenario_instrument_id(scenario: str) -> InstrumentId:
    """
    Map a scenario to the instrument it trades.

    Parameters
    ----------
    scenario : str
        The scenario name.

    Returns
    -------
    InstrumentId

    """
    if scenario in (SCENARIO_MKP, SCENARIO_FUTURES_OCTYPE):
        return FUTURES_INSTRUMENT_ID
    return STOCK_INSTRUMENT_ID


def build_node(scenario: str) -> TradingNode:
    """
    Build the trading node and attach the strategy for the given scenario.

    Parameters
    ----------
    scenario : str
        One of ``SCENARIOS``.

    Returns
    -------
    TradingNode
        A built node ready to run.

    """
    config_node = TradingNodeConfig(
        trader_id=TraderId("TESTER-001"),
        logging=LoggingConfig(log_level="INFO", use_pyo3=True),
        exec_engine=LiveExecEngineConfig(
            reconciliation=True,
        ),
        cache=CacheConfig(
            encoding="msgpack",
            timestamps_as_iso8601=True,
            buffer_interval_ms=100,
        ),
        data_clients={
            SINOPAC: SinopacDataClientConfig(
                gateway_host=GATEWAY_HOST,
                gateway_port=GATEWAY_PORT,
                instrument_provider=InstrumentProviderConfig(load_all=True),
            ),
        },
        exec_clients={
            SINOPAC: SinopacExecClientConfig(
                gateway_host=GATEWAY_HOST,
                gateway_port=GATEWAY_PORT,
                account_id=SINOPAC_ACCOUNT_ID,
                instrument_provider=InstrumentProviderConfig(load_all=True),
            ),
        },
        timeout_connection=30.0,
        timeout_reconciliation=20.0,
        timeout_portfolio=10.0,
        timeout_disconnection=5.0,
        timeout_post_stop=5.0,
    )

    node = TradingNode(config=config_node)
    node.add_data_client_factory(SINOPAC, SinopacLiveDataClientFactory)
    node.add_exec_client_factory(SINOPAC, SinopacLiveExecClientFactory)

    dry_run = _env_dry_run()

    if scenario == SCENARIO_COMMON:
        # Unchanged stock ExecTester behavior.
        config_tester = ExecTesterConfig(
            instrument_id=STOCK_INSTRUMENT_ID,
            external_order_claims=[STOCK_INSTRUMENT_ID],
            order_qty=TRADE_SIZE,
            tob_offset_ticks=OFFSET_TICKS,
            subscribe_quotes=True,
            subscribe_trades=True,
            enable_stop_buys=False,  # Sinopac doesn't support stop orders natively
            enable_stop_sells=False,
            enable_brackets=False,  # Sinopac doesn't support bracket orders
            use_post_only=False,  # Not applicable to Taiwan exchange
            close_positions_time_in_force=TimeInForce.DAY,  # Taiwan uses ROD (rest of day)
            dry_run=dry_run,  # DRY_RUN_DEFAULT (False) unless SINOPAC_EXEC_DRY_RUN set
            log_data=True,
        )
        strategy: Strategy = ExecTester(config=config_tester)
    else:
        instrument_id = _scenario_instrument_id(scenario)
        config_scenario = OrderSemanticsScenarioConfig(
            instrument_id=instrument_id,
            external_order_claims=[instrument_id],
            scenario=scenario,
            dry_run=dry_run,
        )
        strategy = OrderSemanticsScenarioStrategy(config=config_scenario)

    node.trader.add_strategy(strategy)
    node.build()
    return node


def main() -> None:
    """
    Build and run the tester node for the resolved scenario.
    """
    scenario = _resolve_scenario()
    node = build_node(scenario)
    try:
        node.run()
    finally:
        node.dispose()


if __name__ == "__main__":
    main()
