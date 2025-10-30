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

from typing import Final

from nautilus_trader.model.enums import OrderType
from nautilus_trader.model.enums import TimeInForce
from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import Venue


MT5: Final[str] = "MT5"
MT5_VENUE: Final[Venue] = Venue(MT5)
MT5_CLIENT_ID: Final[ClientId] = ClientId(MT5)

# MT5 supports these order types
MT5_SUPPORTED_ORDER_TYPES: Final[set[OrderType]] = {
    OrderType.MARKET,
    OrderType.LIMIT,
    OrderType.STOP_MARKET,
    OrderType.STOP_LIMIT,
}

# MT5 primarily uses GTC (Good Till Cancel)
MT5_SUPPORTED_TIF: Final[set[TimeInForce]] = {
    TimeInForce.GTC,
    TimeInForce.DAY,
    TimeInForce.IOC,  # Immediate Or Cancel
    TimeInForce.FOK,  # Fill Or Kill
}

# MT5 timeframes for bars/historical data
MT5_TIMEFRAME_MAP: Final[dict[str, int]] = {
    "M1": 1,      # 1 minute
    "M5": 5,      # 5 minutes
    "M15": 15,    # 15 minutes
    "M30": 30,    # 30 minutes
    "H1": 60,     # 1 hour
    "H4": 240,    # 4 hours
    "D1": 1440,   # 1 day
    "W1": 10080,  # 1 week
    "MN1": 43200, # 1 month
}

# MT5 trade return codes indicating success (from Rust constants.rs)
MT5_SUCCESS_CODES: Final[set[int]] = {
    10009,  # TRADE_RETCODE_DONE - Order placed successfully
    10008,  # TRADE_RETCODE_PLACED - Order accepted
    10010,  # TRADE_RETCODE_DONE_PARTIAL - Order partially filled
}

# MT5 error codes for which retries might make sense
MT5_RETRY_ERRORS: Final[set[int]] = {
    10004,  # TRADE_RETCODE_REQUOTE - Price changed, retry possible
    10006,  # TRADE_RETCODE_REJECT - Request rejected by server, retry
    10019,  # TRADE_RETCODE_NO_MONEY - Insufficient funds
    10021,  # TRADE_RETCODE_PRICE_OFF - Invalid price
    10027,  # TRADE_RETCODE_AUTOTRADING_DISABLED - Autotrading disabled
    10030,  # TRADE_RETCODE_POSITION_CLOSED - Position already closed
    10034,  # TRADE_RETCODE_LONG_ONLY - Long positions only allowed
    10035,  # TRADE_RETCODE_SHORT_ONLY - Short positions only allowed
}
