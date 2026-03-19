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

from nautilus_trader.adapters.sinopac.factories import get_sinopac_http_client
from nautilus_trader.adapters.sinopac.factories import get_sinopac_instrument_provider
from nautilus_trader.adapters.sinopac.factories import get_sinopac_ws_client
from nautilus_trader.adapters.sinopac.providers import SinopacInstrumentProvider
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.core.nautilus_pyo3 import sinopac as pyo3_sinopac


def test_get_sinopac_http_client():
    # Clear the lru_cache to avoid cross-test pollution
    get_sinopac_http_client.cache_clear()

    client = get_sinopac_http_client()
    assert isinstance(client, pyo3_sinopac.SinopacHttpClient)

    # Second call should return the cached instance
    client2 = get_sinopac_http_client()
    assert client is client2

    get_sinopac_http_client.cache_clear()


def test_get_sinopac_ws_client():
    get_sinopac_ws_client.cache_clear()

    client = get_sinopac_ws_client()
    assert isinstance(client, pyo3_sinopac.SinopacWebSocketClient)

    # Second call should return the cached instance
    client2 = get_sinopac_ws_client()
    assert client is client2

    get_sinopac_ws_client.cache_clear()


def test_get_sinopac_instrument_provider():
    get_sinopac_http_client.cache_clear()
    get_sinopac_instrument_provider.cache_clear()

    http_client = get_sinopac_http_client()
    config = InstrumentProviderConfig()
    provider = get_sinopac_instrument_provider(http_client, config)
    assert isinstance(provider, SinopacInstrumentProvider)

    # Second call should return the cached instance
    provider2 = get_sinopac_instrument_provider(http_client, config)
    assert provider is provider2

    get_sinopac_instrument_provider.cache_clear()
    get_sinopac_http_client.cache_clear()
