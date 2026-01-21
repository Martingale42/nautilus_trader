# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2025 Nautech Systems Pty Ltd. All rights reserved.
#  https://nautechsystems.io
#
#  Licensed under the GNU Lesser General Public License Version 3.0 (the "License");
#  You may not use this file except in compliance with the License.
#  You may obtain a copy of the License at https://www.gnu.org/licenses/lgpl-3.0.en.html
#
#  Unless required by applicable law or agreed to in writing, software
#  distributed under the License is distributed on an "AS IS" BASIS,
#  WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
#  See the License for the specific language governing permissions and
#  limitations under the License.
# -------------------------------------------------------------------------------------------------

import asyncio
from typing import Any

from nautilus_trader.adapters.mt5.config import MT5DataClientConfig
from nautilus_trader.adapters.mt5.constants import MT5
from nautilus_trader.adapters.mt5.providers import MT5InstrumentProvider
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.common.enums import LogColor
from nautilus_trader.core import nautilus_pyo3
from nautilus_trader.data.messages import RequestBars
from nautilus_trader.data.messages import RequestInstrument
from nautilus_trader.data.messages import RequestInstruments
from nautilus_trader.data.messages import RequestQuoteTicks
from nautilus_trader.data.messages import RequestTradeTicks
from nautilus_trader.data.messages import SubscribeBars
from nautilus_trader.data.messages import SubscribeInstrument
from nautilus_trader.data.messages import SubscribeInstruments
from nautilus_trader.data.messages import SubscribeQuoteTicks
from nautilus_trader.data.messages import SubscribeTradeTicks
from nautilus_trader.data.messages import UnsubscribeBars
from nautilus_trader.data.messages import UnsubscribeInstrument
from nautilus_trader.data.messages import UnsubscribeInstruments
from nautilus_trader.data.messages import UnsubscribeQuoteTicks
from nautilus_trader.data.messages import UnsubscribeTradeTicks
from nautilus_trader.live.cancellation import DEFAULT_FUTURE_CANCELLATION_TIMEOUT
from nautilus_trader.live.cancellation import cancel_tasks_with_timeout
from nautilus_trader.live.data_client import LiveMarketDataClient
from nautilus_trader.model.data import Bar
from nautilus_trader.model.data import BarType
from nautilus_trader.model.data import QuoteTick
from nautilus_trader.model.data import capsule_to_data
from nautilus_trader.model.enums import BarAggregation
from nautilus_trader.model.enums import PriceType
from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.instruments import CurrencyPair
from nautilus_trader.model.instruments import Instrument
from nautilus_trader.model.objects import Price
from nautilus_trader.model.objects import Quantity

# MT5 supported timeframes (in minutes)
MT5_TIMEFRAMES = {
    1: "M1",
    5: "M5",
    15: "M15",
    30: "M30",
    60: "H1",
    240: "H4",
    1440: "D1",
    10080: "W1",
    43200: "MN1",
}


def get_mt5_timeframe_from_bar_type(bar_type: BarType) -> str:
    """
    Convert a Nautilus BarType to MT5 timeframe string.

    Parameters
    ----------
    bar_type : BarType
        The bar type to convert.

    Returns
    -------
    str
        The MT5 timeframe string (e.g., "M1", "H1", "D1").

    Raises
    ------
    ValueError
        If the bar aggregation or step is not supported by MT5.

    """
    aggregation: BarAggregation = bar_type.spec.aggregation
    step: int = bar_type.spec.step

    # Convert to minutes based on aggregation type
    match aggregation:
        case BarAggregation.MINUTE:
            minutes = step
        case BarAggregation.HOUR:
            minutes = step * 60
        case BarAggregation.DAY:
            minutes = step * 1440
        case BarAggregation.WEEK:
            minutes = step * 10080
        case BarAggregation.MONTH:
            minutes = step * 43200
        case _:
            raise ValueError(
                f"MT5 does not support {aggregation} bar aggregation. "
                "Supported: MINUTE, HOUR, DAY, WEEK, MONTH"
            )

    # Map to MT5 timeframe
    if minutes not in MT5_TIMEFRAMES:
        raise ValueError(
            f"MT5 does not support {minutes}-minute bars. "
            f"Supported timeframes (in minutes): {list(MT5_TIMEFRAMES.keys())}"
        )

    return MT5_TIMEFRAMES[minutes]


class MT5DataClient(LiveMarketDataClient):
    """
    Provides a data client for the MetaTrader 5 trading platform.

    Parameters
    ----------
    loop : asyncio.AbstractEventLoop
        The event loop for the client.
    client : nautilus_pyo3.Mt5Client
        The MT5 ZeroMQ client.
    msgbus : MessageBus
        The message bus for the client.
    cache : Cache
        The cache for the client.
    clock : LiveClock
        The clock for the client.
    instrument_provider : MT5InstrumentProvider
        The instrument provider.
    config : MT5DataClientConfig
        The configuration for the client.
    name : str, optional
        The custom client ID.

    """

    def __init__(
        self,
        loop: asyncio.AbstractEventLoop,
        client: nautilus_pyo3.Mt5Client,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
        instrument_provider: MT5InstrumentProvider,
        config: MT5DataClientConfig,
        name: str | None,
    ) -> None:
        super().__init__(
            loop=loop,
            client_id=ClientId(name or MT5),
            venue=None,  # Multi-venue (MT5 can connect to multiple brokers)
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=instrument_provider,
        )
        self._instrument_provider: MT5InstrumentProvider = instrument_provider

        # Configuration
        self._config = config
        self._log.info(f"Host: {config.host}:{config.live_port}", LogColor.BLUE)

        # ZeroMQ client
        self._client = client
        self._client_futures: set[asyncio.Future] = set()

        # Subscription tracking
        self._subscribed_quotes: set[InstrumentId] = set()
        self._subscribed_trades: set[InstrumentId] = set()

    @property
    def mt5_instrument_provider(self) -> MT5InstrumentProvider:
        return self._instrument_provider

    async def _connect(self) -> None:
        """
        Connect to MT5 and initialize instruments.

        """
        # Initialize instrument provider (loads from MT5)
        await self._instrument_provider.initialize()
        self._cache_instruments()
        self._send_all_instruments_to_data_engine()

        # Connect ZeroMQ client with callback for incoming messages
        await self._client.connect(
            instruments=self.mt5_instrument_provider.instruments_pyo3(),
            callback=self._handle_msg,
        )

        # Wait for connection
        await self._client.wait_until_active(timeout_secs=10.0)
        self._log.info(f"Connected to MT5-ZeroMQ at {self._config.host}", LogColor.GREEN)

    async def _disconnect(self) -> None:
        """
        Disconnect from MT5.

        """
        # Allow time for pending messages
        await asyncio.sleep(1.0)

        # Close ZeroMQ connection
        if self._client.is_active():
            self._log.info("Closing MT5 connection")
            self._client.close()
            self._log.info(f"Closed MT5-ZeroMQ connection at {self._config.host}", LogColor.BLUE)

        # Cancel pending futures
        await cancel_tasks_with_timeout(
            self._client_futures,
            self._log,
            timeout_secs=DEFAULT_FUTURE_CANCELLATION_TIMEOUT,
        )
        self._client_futures.clear()

    def _cache_instruments(self) -> None:
        """
        Cache instruments for the HTTP client and internal use.

        """
        instruments_pyo3 = self.mt5_instrument_provider.instruments_pyo3()
        for inst in instruments_pyo3:
            self._client.add_instrument(inst)

        self._log.debug(f"Cached {len(instruments_pyo3)} instruments", LogColor.MAGENTA)

    def _cache_instrument(self, instrument: Instrument) -> None:
        """
        Cache a single instrument.

        Parameters
        ----------
        instrument : Instrument
            The instrument to cache.

        """
        self._instrument_provider.add(instrument)
        self._client.add_instrument(instrument)
        self._log.debug(f"Cached instrument {instrument.id}", LogColor.MAGENTA)

    def _send_all_instruments_to_data_engine(self) -> None:
        """
        Send all cached instruments to the data engine.

        """
        for currency in self._instrument_provider.currencies().values():
            self._cache.add_currency(currency)

        for instrument in self._instrument_provider.get_all().values():
            self._handle_data(instrument)

    async def _subscribe_instruments(self, command: SubscribeInstruments) -> None:
        """
        Subscribe to all instruments (no-op for MT5).

        """
        pass  # MT5 doesn't support instrument updates subscription

    async def _subscribe_instrument(self, command: SubscribeInstrument) -> None:
        """
        Subscribe to a single instrument (no-op for MT5).

        """
        pass  # MT5 doesn't support instrument updates subscription

    async def _subscribe_quote_ticks(self, command: SubscribeQuoteTicks) -> None:
        """
        Subscribe to quote ticks for an instrument.

        Parameters
        ----------
        command : SubscribeQuoteTicks
            The subscription command.

        """
        instrument_id = command.instrument_id
        symbol = instrument_id.symbol.value

        if instrument_id in self._subscribed_quotes:
            self._log.warning(f"Already subscribed to {instrument_id} quotes")
            return

        self._subscribed_quotes.add(instrument_id)

        # Subscribe via MT5 ZeroMQ client (sends CONFIG message with TICK timeframe)
        self._client.subscribe_quotes([str(instrument_id)])
        self._log.info(f"Subscribed to {instrument_id} quote ticks", LogColor.BLUE)

    async def _subscribe_trade_ticks(self, command: SubscribeTradeTicks) -> None:
        """
        Subscribe to trade ticks for an instrument.

        MT5 doesn't distinguish between quotes and trades in the same way as exchanges.
        Subscribing to trades will use the same tick stream as quotes.

        Parameters
        ----------
        command : SubscribeTradeTicks
            The subscription command.

        """
        instrument_id = command.instrument_id
        symbol = instrument_id.symbol.value

        if instrument_id in self._subscribed_trades:
            self._log.warning(f"Already subscribed to {instrument_id} trades")
            return

        self._subscribed_trades.add(instrument_id)

        # Subscribe via MT5 ZeroMQ client (uses same tick stream as quotes)
        self._client.subscribe_quotes([symbol])
        self._log.info(f"Subscribed to {instrument_id} trade ticks", LogColor.BLUE)

    async def _subscribe_bars(self, command: SubscribeBars) -> None:
        """
        Subscribe to bar data from MT5.

        Parameters
        ----------
        command : SubscribeBars
            The subscription command.

        """
        bar_type = command.bar_type

        # Validate bar type - MT5 only supports externally aggregated time bars
        if bar_type.is_internally_aggregated():
            self._log.error(
                f"Cannot subscribe to {bar_type} bars: "
                "only EXTERNAL aggregation supported by MT5"
            )
            return

        if not bar_type.spec.is_time_aggregated():
            self._log.error(
                f"Cannot subscribe to {bar_type} bars: "
                "only time-based bars supported by MT5"
            )
            return

        if bar_type.spec.price_type != PriceType.LAST:
            self._log.error(
                f"Cannot subscribe to {bar_type} bars: "
                "only LAST price type supported by MT5"
            )
            return

        # Convert BarType to MT5 timeframe string
        try:
            timeframe_str = get_mt5_timeframe_from_bar_type(bar_type)
        except ValueError as e:
            self._log.error(f"Cannot subscribe to {bar_type} bars: {e}")
            return

        # Subscribe via MT5 ZeroMQ client (pass both symbol, timeframe, and bar_type string)
        symbol = bar_type.instrument_id.symbol.value
        bar_type_str = str(bar_type)
        self._client.subscribe_bars(symbol, timeframe_str, bar_type_str)
        self._log.info(f"Subscribed to {bar_type} bars", LogColor.BLUE)

    async def _subscribe_instrument_status(self, command) -> None:
        """
        Subscribe to instrument status updates (not supported by MT5).

        MT5 does not provide real-time instrument status updates.
        This is a no-op to prevent NotImplementedError.

        """
        self._log.warning(
            f"Instrument status updates not supported by MT5 for {command.instrument_id}"
        )

    async def _subscribe_instrument_close(self, command) -> None:
        """
        Subscribe to instrument close updates (not supported by MT5).

        MT5 does not provide instrument close notifications.
        This is a no-op to prevent NotImplementedError.

        """
        self._log.warning(
            f"Instrument close updates not supported by MT5 for {command.instrument_id}"
        )

    async def _unsubscribe_instruments(self, command: UnsubscribeInstruments) -> None:
        """
        Unsubscribe from all instruments (no-op for MT5).

        """
        pass

    async def _unsubscribe_instrument(self, command: UnsubscribeInstrument) -> None:
        """
        Unsubscribe from a single instrument (no-op for MT5).

        """
        pass

    async def _unsubscribe_quote_ticks(self, command: UnsubscribeQuoteTicks) -> None:
        """
        Unsubscribe from quote ticks for an instrument.

        Parameters
        ----------
        command : UnsubscribeQuoteTicks
            The unsubscription command.

        """
        instrument_id = command.instrument_id

        if instrument_id not in self._subscribed_quotes:
            self._log.warning(f"Not subscribed to {instrument_id} quotes")
            return

        self._subscribed_quotes.discard(instrument_id)
        # Note: MT5 ZeroMQ doesn't have explicit unsubscribe, just stop processing
        self._log.info(f"Unsubscribed from {instrument_id} quote ticks", LogColor.BLUE)

    async def _unsubscribe_trade_ticks(self, command: UnsubscribeTradeTicks) -> None:
        """
        Unsubscribe from trade ticks for an instrument.

        Parameters
        ----------
        command : UnsubscribeTradeTicks
            The unsubscription command.

        """
        instrument_id = command.instrument_id

        if instrument_id not in self._subscribed_trades:
            self._log.warning(f"Not subscribed to {instrument_id} trades")
            return

        self._subscribed_trades.discard(instrument_id)
        self._log.info(f"Unsubscribed from {instrument_id} trade ticks", LogColor.BLUE)

    async def _unsubscribe_bars(self, command: UnsubscribeBars) -> None:
        """
        Unsubscribe from bar data (no-op for MT5).

        """
        pass

    async def _request_instrument(self, request: RequestInstrument) -> None:
        """
        Request a single instrument.

        Parameters
        ----------
        request : RequestInstrument
            The request message.

        """
        instrument = self._instrument_provider.find(request.instrument_id)

        if instrument is None:
            self._log.error(
                f"Cannot find instrument for {request.instrument_id}",
            )
            return

        self._handle_instrument(instrument, request.id)

    async def _request_instruments(self, request: RequestInstruments) -> None:
        """
        Request all instruments.

        Parameters
        ----------
        request : RequestInstruments
            The request message.

        """
        instruments = self._instrument_provider.get_all()

        self._handle_instruments(
            request.venue,
            instruments,
            request.id,
            request.start,
            request.end,
            request.params,
        )

    async def _request_quote_ticks(self, request: RequestQuoteTicks) -> None:
        """
        Request historical quote ticks from MT5.

        Parameters
        ----------
        request : RequestQuoteTicks
            The request message.

        """
        instrument = self._cache.instrument(request.instrument_id)
        if instrument is None:
            self._log.error(
                f"Cannot request quote ticks: no instrument for {request.instrument_id}",
            )
            return

        # Get symbol from instrument
        symbol = request.instrument_id.symbol.value

        # Convert datetime to Unix timestamps (milliseconds for ticks)
        start_ts = None
        end_ts = None
        count = request.limit if request.limit else 1000

        if request.start:
            start_ts = int(request.start.timestamp())
        if request.end:
            end_ts = int(request.end.timestamp())

        self._log.info(
            f"Requesting {count} quote ticks for {request.instrument_id} "
            f"(symbol={symbol}, start={start_ts}, end={end_ts})",
        )

        try:
            # Request ticks from MT5 via Rust client (timeframe="TICK")
            response = await self._client.request_bars(
                symbol=symbol,
                timeframe="TICK",
                start=start_ts,
                end=end_ts,
                count=count,
            )

            # Parse ticks from response
            # MT5 format: {"symbol": str, "timeframe": "TICK", "data": [[timestamp_ms, bid, ask], ...]}
            ticks = []
            for tick_array in response["data"]:
                # Array format: [timestamp_ms, bid, ask]
                if len(tick_array) < 3:
                    self._log.warning(f"Skipping malformed tick data: {tick_array}")
                    continue

                timestamp_ms = int(tick_array[0])
                bid_price_val = float(tick_array[1])
                ask_price_val = float(tick_array[2])

                # Convert to Nautilus QuoteTick using Python model types (following Bybit pattern)
                tick = QuoteTick(
                    instrument_id=request.instrument_id,  # Already correct Python type
                    bid_price=Price(bid_price_val, instrument.price_precision),
                    ask_price=Price(ask_price_val, instrument.price_precision),
                    bid_size=Quantity(0, 0),  # MT5 doesn't provide size in historical ticks
                    ask_size=Quantity(0, 0),  # MT5 doesn't provide size in historical ticks
                    ts_event=timestamp_ms * 1_000_000,  # Convert milliseconds to nanoseconds
                    ts_init=self._clock.timestamp_ns(),
                )
                ticks.append(tick)

            self._log.info(f"Received {len(ticks)} quote ticks from MT5", LogColor.GREEN)

            # Publish ticks via parent class method
            self._handle_quote_ticks(
                request.instrument_id,
                ticks,
                request.id,
                request.start,
                request.end,
                request.params,
            )
        except Exception as e:
            self._log.exception(f"Failed to request quote ticks from MT5", e)

    async def _request_trade_ticks(self, request: RequestTradeTicks) -> None:
        """
        Request historical trade ticks (not supported by MT5).

        """
        self._log.error(
            f"Cannot request historical trades for {request.instrument_id}: "
            "not supported by MT5 adapter (use bars instead)",
        )

    async def _request_bars(self, request: RequestBars) -> None:
        """
        Request historical bar data from MT5.

        Parameters
        ----------
        request : RequestBars
            The request message.

        """
        if request.bar_type.is_internally_aggregated():
            self._log.error(
                f"Cannot request {request.bar_type} bars: "
                "only external aggregation supported by MT5",
            )
            return

        if request.bar_type.spec.price_type != PriceType.LAST:
            self._log.error(
                f"Cannot request {request.bar_type} bars: "
                "only LAST price type supported by MT5",
            )
            return

        if not request.bar_type.spec.is_time_aggregated():
            self._log.error(
                f"Cannot request {request.bar_type} bars: "
                "only time-aggregated bars supported by MT5",
            )
            return

        instrument = self._cache.instrument(request.bar_type.instrument_id)
        if instrument is None:
            self._log.error(
                f"Cannot request bars: no instrument for {request.bar_type.instrument_id}",
            )
            return

        try:
            # Get MT5 timeframe
            mt5_timeframe = get_mt5_timeframe_from_bar_type(request.bar_type)
        except ValueError as e:
            self._log.error(f"Cannot request bars: {e}")
            return

        # Get symbol from instrument
        symbol = request.bar_type.instrument_id.symbol.value

        # Convert datetime to Unix timestamps (seconds)
        start_ts = None
        end_ts = None
        count = request.limit if request.limit else 1000

        if request.start:
            start_ts = int(request.start.timestamp())
        if request.end:
            end_ts = int(request.end.timestamp())

        self._log.info(
            f"Requesting {count} bars for {request.bar_type} "
            f"(symbol={symbol}, timeframe={mt5_timeframe}, "
            f"start={start_ts}, end={end_ts})",
        )

        try:
            # Request bars from MT5 via Rust client
            response = await self._client.request_bars(
                symbol=symbol,
                timeframe=mt5_timeframe,
                start=start_ts,
                end=end_ts,
                count=count,
            )

            # Parse bars from response
            # MT5 format: {"symbol": str, "timeframe": str, "data": [[timestamp, open, high, low, close, volume], ...]}
            bars = []
            for bar_array in response["data"]:
                # Array format: [timestamp_sec, open, high, low, close, volume]
                if len(bar_array) < 6:
                    self._log.warning(f"Skipping malformed bar data: {bar_array}")
                    continue

                timestamp_sec = int(bar_array[0])
                open_price_val = float(bar_array[1])
                high_price_val = float(bar_array[2])
                low_price_val = float(bar_array[3])
                close_price_val = float(bar_array[4])
                volume_val = float(bar_array[5])

                # Convert to Nautilus Bar using Python model types (following Bybit pattern)
                bar = Bar(
                    bar_type=request.bar_type,  # Already correct Python type
                    open=Price(open_price_val, instrument.price_precision),
                    high=Price(high_price_val, instrument.price_precision),
                    low=Price(low_price_val, instrument.price_precision),
                    close=Price(close_price_val, instrument.price_precision),
                    volume=Quantity(volume_val, instrument.size_precision),
                    ts_event=timestamp_sec * 1_000_000_000,  # Convert seconds to nanoseconds
                    ts_init=self._clock.timestamp_ns(),
                )
                bars.append(bar)

            self._log.info(f"Received {len(bars)} bars from MT5", LogColor.GREEN)

            # Publish bars via parent class method
            self._handle_bars(
                request.bar_type,
                bars,
                request.id,
                request.start,
                request.end,
                request.params,
            )
        except Exception as e:
            self._log.exception(f"Failed to request bars from MT5", e)

    def _handle_msg(self, msg: Any) -> None:
        """
        Handle incoming messages from MT5 ZeroMQ client.

        Parameters
        ----------
        msg : Any
            The message from the Rust client (PyCapsule or pyo3 type).

        """
        try:
            # Handle PyCapsule data (QuoteTick, TradeTick, etc.)
            if nautilus_pyo3.is_pycapsule(msg):
                data = capsule_to_data(msg)
            # Handle pyo3 instrument updates
            elif isinstance(msg, nautilus_pyo3.CurrencyPair):
                self._cache_instrument(msg)
                data = CurrencyPair.from_pyo3(msg)
            else:
                self._log.error(f"Cannot handle message type {type(msg)}, not implemented")
                return

            # Route to data engine
            self._handle_data(data)
        except Exception as e:
            self._log.exception("Error handling MT5 message", e)
