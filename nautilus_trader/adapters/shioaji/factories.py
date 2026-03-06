import asyncio
from functools import lru_cache

from nautilus_trader.adapters.shioaji.config import ShioajiDataClientConfig
from nautilus_trader.adapters.shioaji.config import ShioajiExecClientConfig
from nautilus_trader.adapters.shioaji.data import ShioajiDataClient
from nautilus_trader.adapters.shioaji.execution import ShioajiExecutionClient
from nautilus_trader.adapters.shioaji.providers import ShioajiInstrumentProvider
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.core.nautilus_pyo3 import shioaji as pyo3_shioaji
from nautilus_trader.live.factories import LiveDataClientFactory
from nautilus_trader.live.factories import LiveExecClientFactory


@lru_cache(1)
def get_shioaji_http_client(
    gateway_host: str = "localhost",
    gateway_port: int = 8000,
) -> pyo3_shioaji.ShioajiHttpClient:
    """
    Cache and return a Shioaji gateway HTTP client.

    Parameters
    ----------
    gateway_host : str, default "localhost"
        The Shioaji gateway host address.
    gateway_port : int, default 8000
        The Shioaji gateway HTTP/WS port.

    Returns
    -------
    pyo3_shioaji.ShioajiHttpClient

    """
    base_url = f"http://{gateway_host}:{gateway_port}"
    return pyo3_shioaji.ShioajiHttpClient(base_url=base_url)


@lru_cache(1)
def get_shioaji_ws_client(
    gateway_host: str = "localhost",
    gateway_port: int = 8000,
    gateway_ws_path: str = "/ws",
) -> pyo3_shioaji.ShioajiWebSocketClient:
    """
    Cache and return a Shioaji gateway WebSocket client.

    Parameters
    ----------
    gateway_host : str, default "localhost"
        The Shioaji gateway host address.
    gateway_port : int, default 8000
        The Shioaji gateway HTTP/WS port.
    gateway_ws_path : str, default "/ws"
        The WebSocket endpoint path.

    Returns
    -------
    pyo3_shioaji.ShioajiWebSocketClient

    """
    ws_url = f"ws://{gateway_host}:{gateway_port}{gateway_ws_path}"
    return pyo3_shioaji.ShioajiWebSocketClient(url=ws_url)


@lru_cache(1)
def get_shioaji_instrument_provider(
    client: pyo3_shioaji.ShioajiHttpClient,
    config: InstrumentProviderConfig,
) -> ShioajiInstrumentProvider:
    """
    Cache and return a Shioaji instrument provider.

    Parameters
    ----------
    client : pyo3_shioaji.ShioajiHttpClient
        The Shioaji gateway HTTP client.
    config : InstrumentProviderConfig
        The configuration for the instrument provider.

    Returns
    -------
    ShioajiInstrumentProvider

    """
    return ShioajiInstrumentProvider(
        client=client,
        config=config,
    )


# ---------------------------------------------------------------------------
# WS callback dispatch — single WS connection shared by Data + Exec clients
# ---------------------------------------------------------------------------


@lru_cache(1)
def _get_ws_msg_handlers() -> list:
    """Return the shared list of WS message handlers."""
    return []


def _ws_dispatch_callback(msg) -> None:
    """Dispatch a WS message to all registered handlers."""
    for handler in _get_ws_msg_handlers():
        handler(msg)


# ---------------------------------------------------------------------------
# Factory classes
# ---------------------------------------------------------------------------


class ShioajiLiveDataClientFactory(LiveDataClientFactory):
    """
    Provides a Shioaji live data client factory.
    """

    @staticmethod
    def create(  # type: ignore
        loop: asyncio.AbstractEventLoop,
        name: str,
        config: ShioajiDataClientConfig,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
    ) -> ShioajiDataClient:
        """
        Create a new Shioaji data client.

        Parameters
        ----------
        loop : asyncio.AbstractEventLoop
            The event loop for the client.
        name : str
            The custom client ID.
        config : ShioajiDataClientConfig
            The client configuration.
        msgbus : MessageBus
            The message bus for the client.
        cache : Cache
            The cache for the client.
        clock : LiveClock
            The clock for the client.

        Returns
        -------
        ShioajiDataClient

        """
        http_client = get_shioaji_http_client(
            gateway_host=config.gateway_host,
            gateway_port=config.gateway_port,
        )
        ws_client = get_shioaji_ws_client(
            gateway_host=config.gateway_host,
            gateway_port=config.gateway_port,
            gateway_ws_path=config.gateway_ws_path,
        )
        provider = get_shioaji_instrument_provider(
            client=http_client,
            config=config.instrument_provider,
        )

        client = ShioajiDataClient(
            loop=loop,
            client=http_client,
            ws_client=ws_client,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=provider,
            config=config,
            name=name,
            ws_callback=_ws_dispatch_callback,
        )

        # Register this client's handler for WS dispatch
        handlers = _get_ws_msg_handlers()
        handlers.append(client._handle_msg)

        return client


class ShioajiLiveExecClientFactory(LiveExecClientFactory):
    """
    Provides a Shioaji live execution client factory.
    """

    @staticmethod
    def create(  # type: ignore
        loop: asyncio.AbstractEventLoop,
        name: str,
        config: ShioajiExecClientConfig,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
    ) -> ShioajiExecutionClient:
        """
        Create a new Shioaji execution client.

        Parameters
        ----------
        loop : asyncio.AbstractEventLoop
            The event loop for the client.
        name : str
            The custom client ID.
        config : ShioajiExecClientConfig
            The client configuration.
        msgbus : MessageBus
            The message bus for the client.
        cache : Cache
            The cache for the client.
        clock : LiveClock
            The clock for the client.

        Returns
        -------
        ShioajiExecutionClient

        """
        http_client = get_shioaji_http_client(
            gateway_host=config.gateway_host,
            gateway_port=config.gateway_port,
        )
        ws_client = get_shioaji_ws_client(
            gateway_host=config.gateway_host,
            gateway_port=config.gateway_port,
            gateway_ws_path=config.gateway_ws_path,
        )
        provider = get_shioaji_instrument_provider(
            client=http_client,
            config=config.instrument_provider,
        )

        client = ShioajiExecutionClient(
            loop=loop,
            client=http_client,
            ws_client=ws_client,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=provider,
            config=config,
            name=name,
        )

        # Register this client's handler for WS dispatch
        handlers = _get_ws_msg_handlers()
        handlers.append(client._handle_msg)

        return client
