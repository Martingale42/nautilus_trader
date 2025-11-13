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
from functools import lru_cache

from nautilus_trader.adapters.mt5.config import MT5DataClientConfig
from nautilus_trader.adapters.mt5.config import MT5ExecClientConfig
from nautilus_trader.adapters.mt5.data import MT5DataClient
from nautilus_trader.adapters.mt5.execution import MT5ExecutionClient
from nautilus_trader.adapters.mt5.providers import MT5InstrumentProvider
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.core import nautilus_pyo3
from nautilus_trader.live.factories import LiveDataClientFactory
from nautilus_trader.live.factories import LiveExecClientFactory


@lru_cache(1)
def get_mt5_client(
    host: str = "localhost",
    data_port: int = 2202,
    live_port: int = 2203,
    stream_port: int = 2204,
    sys_port: int = 2201,
    account_id: str | None = None,
) -> nautilus_pyo3.Mt5Client:
    """
    Cache and return an MT5 ZeroMQ client with the given configuration.

    If a cached client with matching configuration already exists, then that cached
    client will be returned.

    Parameters
    ----------
    host : str, default "localhost"
        The ZeroMQ host address.
    data_port : int, default 2202
        The data port (command responses).
    live_port : int, default 2203
        The live data port (tick streaming).
    stream_port : int, default 2204
        The stream port (orders/positions).
    sys_port : int, default 2201
        The system port (command requests).
    account_id : str, optional
        The MT5 account identifier.

    Returns
    -------
    nautilus_pyo3.Mt5Client
        The cached MT5 ZeroMQ client.

    """
    return nautilus_pyo3.Mt5Client(
        host=host,
        data_port=data_port,
        live_port=live_port,
        stream_port=stream_port,
        sys_port=sys_port,
        account_id=account_id,
    )


@lru_cache(1)
def get_mt5_instrument_provider(
    client: nautilus_pyo3.Mt5Client,
    config: InstrumentProviderConfig,
) -> MT5InstrumentProvider:
    """
    Cache and return an MT5 instrument provider.

    If a cached provider already exists, then that provider will be returned.

    Parameters
    ----------
    client : nautilus_pyo3.Mt5Client
        The MT5 ZeroMQ client for the instrument provider.
    config : InstrumentProviderConfig
        The configuration for the instrument provider.

    Returns
    -------
    MT5InstrumentProvider
        The cached MT5 instrument provider.

    """
    return MT5InstrumentProvider(
        client=client,
        config=config,
    )


class MT5LiveDataClientFactory(LiveDataClientFactory):
    """
    Provides an MT5 live data client factory.
    """

    @staticmethod
    def create(  # type: ignore
        loop: asyncio.AbstractEventLoop,
        name: str,
        config: MT5DataClientConfig,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
    ) -> MT5DataClient:
        """
        Create a new MT5 data client.

        Parameters
        ----------
        loop : asyncio.AbstractEventLoop
            The event loop for the client.
        name : str
            The custom client ID.
        config : MT5DataClientConfig
            The client configuration.
        msgbus : MessageBus
            The message bus for the client.
        cache : Cache
            The cache for the client.
        clock : LiveClock
            The clock for the client.

        Returns
        -------
        MT5DataClient
            The created MT5 data client.

        """
        client: nautilus_pyo3.Mt5Client = get_mt5_client(
            host=config.host,
            data_port=config.data_port,
            live_port=config.live_port,
            stream_port=config.stream_port,
            sys_port=config.sys_port,
            account_id=config.account_id,
        )
        provider = get_mt5_instrument_provider(
            client=client,
            config=config.instrument_provider,
        )

        return MT5DataClient(
            loop=loop,
            client=client,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=provider,
            config=config,
            name=name,
        )


class MT5LiveExecClientFactory(LiveExecClientFactory):
    """
    Provides an MT5 live execution client factory.
    """

    @staticmethod
    def create(  # type: ignore
        loop: asyncio.AbstractEventLoop,
        name: str,
        config: MT5ExecClientConfig,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
    ) -> MT5ExecutionClient:
        """
        Create a new MT5 execution client.

        Parameters
        ----------
        loop : asyncio.AbstractEventLoop
            The event loop for the client.
        name : str
            The custom client ID.
        config : MT5ExecClientConfig
            The client configuration.
        msgbus : MessageBus
            The message bus for the client.
        cache : Cache
            The cache for the client.
        clock : LiveClock
            The clock for the client.

        Returns
        -------
        MT5ExecutionClient
            The created MT5 execution client.

        """
        client: nautilus_pyo3.Mt5Client = get_mt5_client(
            host=config.host,
            data_port=config.data_port,
            live_port=2203,  # Not needed for execution, but required param
            stream_port=config.stream_port,
            sys_port=config.sys_port,
            account_id=config.account_id,
        )
        provider = get_mt5_instrument_provider(
            client=client,
            config=config.instrument_provider,
        )

        return MT5ExecutionClient(
            loop=loop,
            client=client,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            instrument_provider=provider,
            config=config,
            name=name,
        )
