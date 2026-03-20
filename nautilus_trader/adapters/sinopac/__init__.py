# -------------------------------------------------------------------------------------------------
#  Copyright (C) 2015-2026 Nautech Systems Pty Ltd. All rights reserved.
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

"""Provides a trading adapter for Sinopac (SinoPac Securities)."""

from nautilus_trader.adapters.sinopac.config import SinopacDataClientConfig
from nautilus_trader.adapters.sinopac.config import SinopacExecClientConfig
from nautilus_trader.adapters.sinopac.constants import SINOPAC
from nautilus_trader.adapters.sinopac.constants import SINOPAC_CLIENT_ID
from nautilus_trader.adapters.sinopac.constants import SINOPAC_VENUE
from nautilus_trader.adapters.sinopac.data import SinopacDataClient
from nautilus_trader.adapters.sinopac.execution import SinopacExecutionClient
from nautilus_trader.adapters.sinopac.factories import SinopacLiveDataClientFactory
from nautilus_trader.adapters.sinopac.factories import SinopacLiveExecClientFactory
from nautilus_trader.adapters.sinopac.factories import get_sinopac_http_client
from nautilus_trader.adapters.sinopac.factories import get_sinopac_instrument_provider
from nautilus_trader.adapters.sinopac.factories import get_sinopac_ws_client
from nautilus_trader.adapters.sinopac.providers import SinopacInstrumentProvider


__all__ = [
    "SINOPAC",
    "SINOPAC_CLIENT_ID",
    "SINOPAC_VENUE",
    "SinopacDataClient",
    "SinopacDataClientConfig",
    "SinopacExecClientConfig",
    "SinopacExecutionClient",
    "SinopacInstrumentProvider",
    "SinopacLiveDataClientFactory",
    "SinopacLiveExecClientFactory",
    "get_sinopac_http_client",
    "get_sinopac_instrument_provider",
    "get_sinopac_ws_client",
]
