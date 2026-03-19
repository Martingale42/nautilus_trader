import asyncio
from functools import lru_cache

from nautilus_trader.adapters.sinopac.config import SinopacDataClientConfig
from nautilus_trader.adapters.sinopac.config import SinopacExecClientConfig
from nautilus_trader.adapters.sinopac.data import SinopacDataClient
from nautilus_trader.adapters.sinopac.execution import SinopacExecutionClient
from nautilus_trader.adapters.sinopac.providers import SinopacInstrumentProvider
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.core.nautilus_pyo3 import sinopac as pyo3_sinopac
from nautilus_trader.live.factories import LiveDataClientFactory
from nautilus_trader.live.factories import LiveExecClientFactory


@lru_cache(1)
def get_sinopac_http_client(
    gateway_host: str = "localhost",
    gateway_port: int = 8000,
) -> pyo3_sinopac.SinopacHttpClient:
    """
    Cache and return a Sinopac gateway HTTP client.

    Parameters
    ----------
    gateway_host : str, default "localhost"
        The Sinopac gateway host address.
    gateway_port : int, default 8000
        The Sinopac gateway HTTP/WS port.

    Returns
    -------
    pyo3_sinopac.SinopacHttpClient

    """
    base_url = f"http://{gateway_host}:{gateway_port}"
    return pyo3_sinopac.SinopacHttpClient(base_url=base_url)


@lru_cache(1)
def get_sinopac_ws_client(
    gateway_host: str = "localhost",
    gateway_port: int = 8000,
    gateway_ws_path: str = "/ws",
) -> pyo3_sinopac.SinopacWebSocketClient:
    """
    Cache and return a Sinopac gateway WebSocket client.

    Parameters
    ----------
    gateway_host : str, default "localhost"
        The Sinopac gateway host address.
    gateway_port : int, default 8000
        The Sinopac gateway HTTP/WS port.
    gateway_ws_path : str, default "/ws"
        The WebSocket endpoint path.

    Returns
    -------
    pyo3_sinopac.SinopacWebSocketClient

    """
    ws_url = f"ws://{gateway_host}:{gateway_port}{gateway_ws_path}"
    return pyo3_sinopac.SinopacWebSocketClient(url=ws_url)


@lru_cache(1)
def get_sinopac_instrument_provider(
    client: pyo3_sinopac.SinopacHttpClient,
    config: InstrumentProviderConfig,
) -> SinopacInstrumentProvider:
    """
    Cache and return a Sinopac instrument provider.

    Parameters
    ----------
    client : pyo3_sinopac.SinopacHttpClient
        The Sinopac gateway HTTP client.
    config : InstrumentProviderConfig
        The configuration for the instrument provider.

    Returns
    -------
    SinopacInstrumentProvider

    """
    return SinopacInstrumentProvider(
        client=client,
        config=config,
    )


# ---------------------------------------------------------------------------
# WS callback dispatch — single WS connection shared by Data + Exec clients
# ---------------------------------------------------------------------------


@lru_cache(1)
def _get_ws_msg_handlers() -> list:
    return []


def _ws_dispatch_callback(msg: object) -> None:
    for handler in _get_ws_msg_handlers():
        handler(msg)


# ---------------------------------------------------------------------------
# Factory classes
# ---------------------------------------------------------------------------


class SinopacLiveDataClientFactory(LiveDataClientFactory):
    """
    Provides a Sinopac live data client factory.
    """

    @staticmethod
    def create(  # type: ignore
        loop: asyncio.AbstractEventLoop,
        name: str,
        config: SinopacDataClientConfig,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
    ) -> SinopacDataClient:
        """
        Create a new Sinopac data client.

        Parameters
        ----------
        loop : asyncio.AbstractEventLoop
            The event loop for the client.
        name : str
            The custom client ID.
        config : SinopacDataClientConfig
            The client configuration.
        msgbus : MessageBus
            The message bus for the client.
        cache : Cache
            The cache for the client.
        clock : LiveClock
            The clock for the client.

        Returns
        -------
        SinopacDataClient

        """
        http_client = get_sinopac_http_client(
            gateway_host=config.gateway_host,
            gateway_port=config.gateway_port,
        )
        ws_client = get_sinopac_ws_client(
            gateway_host=config.gateway_host,
            gateway_port=config.gateway_port,
            gateway_ws_path=config.gateway_ws_path,
        )
        provider = get_sinopac_instrument_provider(
            client=http_client,
            config=config.instrument_provider,
        )

        client = SinopacDataClient(
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


class SinopacLiveExecClientFactory(LiveExecClientFactory):
    """
    Provides a Sinopac live execution client factory.
    """

    @staticmethod
    def create(  # type: ignore
        loop: asyncio.AbstractEventLoop,
        name: str,
        config: SinopacExecClientConfig,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
    ) -> SinopacExecutionClient:
        """
        Create a new Sinopac execution client.

        Parameters
        ----------
        loop : asyncio.AbstractEventLoop
            The event loop for the client.
        name : str
            The custom client ID.
        config : SinopacExecClientConfig
            The client configuration.
        msgbus : MessageBus
            The message bus for the client.
        cache : Cache
            The cache for the client.
        clock : LiveClock
            The clock for the client.

        Returns
        -------
        SinopacExecutionClient

        """
        http_client = get_sinopac_http_client(
            gateway_host=config.gateway_host,
            gateway_port=config.gateway_port,
        )
        ws_client = get_sinopac_ws_client(
            gateway_host=config.gateway_host,
            gateway_port=config.gateway_port,
            gateway_ws_path=config.gateway_ws_path,
        )
        provider = get_sinopac_instrument_provider(
            client=http_client,
            config=config.instrument_provider,
        )

        client = SinopacExecutionClient(
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
