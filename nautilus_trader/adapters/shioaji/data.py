import asyncio

from nautilus_trader.adapters.shioaji.config import ShioajiDataClientConfig
from nautilus_trader.adapters.shioaji.constants import SINOPAC
from nautilus_trader.adapters.shioaji.constants import SINOPAC_VENUE
from nautilus_trader.adapters.shioaji.providers import ShioajiInstrumentProvider
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.common.enums import LogColor
from nautilus_trader.core import nautilus_pyo3
from nautilus_trader.core.nautilus_pyo3 import shioaji as pyo3_shioaji
from nautilus_trader.data.messages import RequestBars
from nautilus_trader.data.messages import RequestQuoteTicks
from nautilus_trader.data.messages import RequestTradeTicks
from nautilus_trader.data.messages import SubscribeQuoteTicks
from nautilus_trader.data.messages import SubscribeTradeTicks
from nautilus_trader.data.messages import UnsubscribeQuoteTicks
from nautilus_trader.data.messages import UnsubscribeTradeTicks
from nautilus_trader.live.cancellation import DEFAULT_FUTURE_CANCELLATION_TIMEOUT
from nautilus_trader.live.cancellation import cancel_tasks_with_timeout
from nautilus_trader.live.data_client import LiveMarketDataClient
from nautilus_trader.model.data import Bar
from nautilus_trader.model.data import TradeTick
from nautilus_trader.model.data import capsule_to_data
from nautilus_trader.model.enums import PriceType
from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import InstrumentId


class ShioajiDataClient(LiveMarketDataClient):
    """
    Provides a data client for the Shioaji (SinoPac) adapter.

    Parameters
    ----------
    loop : asyncio.AbstractEventLoop
        The event loop for the client.
    client : pyo3_shioaji.ShioajiHttpClient
        The Shioaji gateway HTTP client.
    ws_client : pyo3_shioaji.ShioajiWebSocketClient
        The Shioaji gateway WebSocket client.
    msgbus : MessageBus
        The message bus for the client.
    cache : Cache
        The cache for the client.
    clock : LiveClock
        The clock for the client.
    instrument_provider : ShioajiInstrumentProvider
        The instrument provider.
    config : ShioajiDataClientConfig
        The configuration for the client.
    name : str, optional
        The custom client ID.

    """

    def __init__(
        self,
        loop: asyncio.AbstractEventLoop,
        client: pyo3_shioaji.ShioajiHttpClient,
        ws_client: pyo3_shioaji.ShioajiWebSocketClient,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
        instrument_provider: ShioajiInstrumentProvider,
        config: ShioajiDataClientConfig,
        name: str | None = None,
    ) -> None:
        super().__init__(
            loop=loop,
            client_id=ClientId(name or SINOPAC),
            venue=SINOPAC_VENUE,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=instrument_provider,
            config=config,
        )
        self._http_client = client
        self._ws_client = ws_client
        self._config = config

        # Subscription tracking
        self._subscribed_trades: set[InstrumentId] = set()
        self._subscribed_quotes: set[InstrumentId] = set()

        # Background task tracking
        self._client_futures: set[asyncio.Future] = set()

    @property
    def shioaji_instrument_provider(self) -> ShioajiInstrumentProvider:
        return self._instrument_provider  # type: ignore

    # -- Connection lifecycle -------------------------------------------------

    async def _connect(self) -> None:
        # 1. Load instruments
        await self._instrument_provider.initialize()
        self._send_all_instruments_to_data_engine()

        # 2. Connect WS with callback
        instruments_pyo3 = self.shioaji_instrument_provider.instruments_pyo3()
        await self._ws_client.connect(
            instruments=instruments_pyo3,
            callback=self._handle_msg,
        )
        await self._ws_client.wait_until_active(timeout_secs=10.0)

        self._log.info(
            f"Connected to Shioaji gateway at {self._config.gateway_base_url}",
            LogColor.GREEN,
        )

    async def _disconnect(self) -> None:
        await asyncio.sleep(1.0)  # Grace period for pending WS messages

        if self._ws_client.is_connected():
            self._log.info("Disconnecting Shioaji WebSocket")
            await self._ws_client.disconnect()
            self._log.info("Shioaji WebSocket disconnected", LogColor.BLUE)

        await cancel_tasks_with_timeout(
            self._client_futures,
            self._log,
            timeout_secs=DEFAULT_FUTURE_CANCELLATION_TIMEOUT,
        )
        self._client_futures.clear()

    def _send_all_instruments_to_data_engine(self) -> None:
        for currency in self._instrument_provider.currencies().values():
            self._cache.add_currency(currency)
        for instrument in self._instrument_provider.get_all().values():
            self._handle_data(instrument)

    def _handle_msg(self, msg) -> None:
        """Handle incoming messages from Shioaji WS (called from Rust)."""
        try:
            if nautilus_pyo3.is_pycapsule(msg):
                data = capsule_to_data(msg)
                self._handle_data(data)
                return
            self._log.warning(f"Unhandled WS message type: {type(msg)}")
        except Exception as e:
            self._log.exception("Error handling Shioaji WS message", e)

    # -- Subscriptions --------------------------------------------------------

    async def _subscribe_trade_ticks(self, command: SubscribeTradeTicks) -> None:
        instrument_id = command.instrument_id
        if instrument_id in self._subscribed_trades:
            self._log.warning(f"Already subscribed to {instrument_id} trades")
            return

        self._subscribed_trades.add(instrument_id)
        code = instrument_id.symbol.value
        self._ws_client.subscribe(code, "tick")
        self._log.info(f"Subscribed to trade ticks: {instrument_id}", LogColor.BLUE)

    async def _unsubscribe_trade_ticks(self, command: UnsubscribeTradeTicks) -> None:
        instrument_id = command.instrument_id
        if instrument_id not in self._subscribed_trades:
            self._log.warning(f"Not subscribed to {instrument_id} trades")
            return

        self._subscribed_trades.discard(instrument_id)
        code = instrument_id.symbol.value
        self._ws_client.unsubscribe(code, "tick")
        self._log.info(f"Unsubscribed from trade ticks: {instrument_id}", LogColor.BLUE)

    async def _subscribe_quote_ticks(self, command: SubscribeQuoteTicks) -> None:
        instrument_id = command.instrument_id
        if instrument_id in self._subscribed_quotes:
            self._log.warning(f"Already subscribed to {instrument_id} quotes")
            return

        self._subscribed_quotes.add(instrument_id)
        code = instrument_id.symbol.value
        self._ws_client.subscribe(code, "bidask")
        self._log.info(f"Subscribed to quote ticks: {instrument_id}", LogColor.BLUE)

    async def _unsubscribe_quote_ticks(self, command: UnsubscribeQuoteTicks) -> None:
        instrument_id = command.instrument_id
        if instrument_id not in self._subscribed_quotes:
            self._log.warning(f"Not subscribed to {instrument_id} quotes")
            return

        self._subscribed_quotes.discard(instrument_id)
        code = instrument_id.symbol.value
        self._ws_client.unsubscribe(code, "bidask")
        self._log.info(f"Unsubscribed from quote ticks: {instrument_id}", LogColor.BLUE)

    async def _subscribe_bars(self, command) -> None:
        self._log.error(
            f"Cannot subscribe to {command.bar_type} bars: "
            "Shioaji does not support streaming bars (use request_bars for historical)",
        )

    async def _unsubscribe_bars(self, command) -> None:
        pass  # No-op

    async def _subscribe_instrument_status(self, command) -> None:
        pass  # Not supported by Shioaji

    async def _subscribe_instrument_close(self, command) -> None:
        pass  # Not supported by Shioaji

    async def _unsubscribe_instrument_status(self, command) -> None:
        pass  # No-op

    async def _unsubscribe_instrument_close(self, command) -> None:
        pass  # No-op

    # -- Historical data requests ---------------------------------------------

    async def _request_trade_ticks(self, request: RequestTradeTicks) -> None:
        instrument_id = request.instrument_id
        instrument = self._cache.instrument(instrument_id)
        if instrument is None:
            self._log.error(f"Cannot find instrument for {instrument_id}")
            return

        code = instrument_id.symbol.value
        date_str = request.start.strftime("%Y-%m-%d") if request.start else None
        if date_str is None:
            self._log.error("request_trade_ticks requires a start date")
            return

        try:
            pyo3_trades = await self._http_client.request_trade_ticks(
                code=code,
                date=date_str,
                price_precision=instrument.price_precision,
                size_precision=instrument.size_precision,
            )
            trades = TradeTick.from_pyo3_list(pyo3_trades)

            self._handle_trade_ticks(
                instrument_id,
                trades,
                request.id,
                request.start,
                request.end,
                request.params,
            )
        except Exception as e:
            self._log.exception("Failed to request trade ticks from Shioaji", e)

    async def _request_bars(self, request: RequestBars) -> None:
        if request.bar_type.is_internally_aggregated():
            self._log.error(
                f"Cannot request {request.bar_type} bars: "
                "only EXTERNAL aggregation supported",
            )
            return

        if not request.bar_type.spec.is_time_aggregated():
            self._log.error(
                f"Cannot request {request.bar_type} bars: "
                "only time bars supported",
            )
            return

        if request.bar_type.spec.price_type != PriceType.LAST:
            self._log.error(
                f"Cannot request {request.bar_type} bars: "
                "only LAST price type supported",
            )
            return

        instrument_id = request.bar_type.instrument_id
        instrument = self._cache.instrument(instrument_id)
        if instrument is None:
            self._log.error(f"Cannot find instrument for {instrument_id}")
            return

        code = instrument_id.symbol.value
        start_str = request.start.strftime("%Y-%m-%d") if request.start else None
        end_str = request.end.strftime("%Y-%m-%d") if request.end else None

        if not start_str or not end_str:
            self._log.error("request_bars requires start and end dates")
            return

        try:
            pyo3_bars = await self._http_client.request_bars(
                code=code,
                start=start_str,
                end=end_str,
                bar_type=str(request.bar_type),
                price_precision=instrument.price_precision,
                size_precision=instrument.size_precision,
            )
            bars = Bar.from_pyo3_list(pyo3_bars)

            self._handle_bars(
                request.bar_type,
                bars,
                request.id,
                request.start,
                request.end,
                request.params,
            )
        except Exception as e:
            self._log.exception("Failed to request bars from Shioaji", e)

    async def _request_quote_ticks(self, request: RequestQuoteTicks) -> None:
        self._log.error("Shioaji does not support historical quote tick requests")
