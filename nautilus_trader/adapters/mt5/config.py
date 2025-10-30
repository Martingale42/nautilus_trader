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

from nautilus_trader.adapters.mt5.constants import MT5_VENUE
from nautilus_trader.config import LiveDataClientConfig
from nautilus_trader.config import LiveExecClientConfig
from nautilus_trader.config import PositiveInt
from nautilus_trader.model.identifiers import Venue


class MT5DataClientConfig(LiveDataClientConfig, frozen=True):
    """
    Configuration for ``MT5DataClient`` instances.

    Parameters
    ----------
    venue : Venue, default MT5_VENUE
        The venue for the client.
    host : str, default "localhost"
        The ZeroMQ host address for MT5-ZeroMQ connection.
    live_port : PositiveInt, default 2203
        The live data port (tick data streaming).
    stream_port : PositiveInt, default 2204
        The stream port (orders/positions updates).
    sys_port : PositiveInt, default 2201
        The system port (command/response).
    account_id : str
        The MT5 account identifier.
        If ``None`` then will source the `MT5_ACCOUNT_ID` environment variable.
    update_instruments_interval_mins : PositiveInt or None, default 60
        The interval (minutes) between instrument list updates.
        If ``None`` then instruments are loaded once on startup.

    """

    venue: Venue = MT5_VENUE
    host: str = "localhost"
    live_port: PositiveInt = 2203
    stream_port: PositiveInt = 2204
    sys_port: PositiveInt = 2201
    account_id: str | None = None
    update_instruments_interval_mins: PositiveInt | None = 60


class MT5ExecClientConfig(LiveExecClientConfig, frozen=True):
    """
    Configuration for ``MT5ExecutionClient`` instances.

    Parameters
    ----------
    venue : Venue, default MT5_VENUE
        The venue for the client.
    host : str, default "localhost"
        The ZeroMQ host address for MT5-ZeroMQ connection.
    stream_port : PositiveInt, default 2204
        The stream port (orders/positions updates).
    sys_port : PositiveInt, default 2201
        The system port (command/response).
    account_id : str
        The MT5 account identifier.
        If ``None`` then will source the `MT5_ACCOUNT_ID` environment variable.
    max_retries : PositiveInt or None, default 3
        The maximum number of retries for failed order operations.
        If ``None`` then will not retry failed operations.
    retry_delay_secs : PositiveInt or None, default 1
        The delay (seconds) between retries.

    """

    venue: Venue = MT5_VENUE
    host: str = "localhost"
    stream_port: PositiveInt = 2204
    sys_port: PositiveInt = 2201
    account_id: str | None = None
    max_retries: PositiveInt | None = 3
    retry_delay_secs: PositiveInt | None = 1
