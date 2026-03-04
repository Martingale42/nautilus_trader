from typing import Final

from nautilus_trader.model.identifiers import ClientId
from nautilus_trader.model.identifiers import Venue


SINOPAC: Final[str] = "SINOPAC"
SINOPAC_VENUE: Final[Venue] = Venue(SINOPAC)
SINOPAC_CLIENT_ID: Final[ClientId] = ClientId(SINOPAC)

SHIOAJI_GATEWAY_URL: Final[str] = "http://localhost:8000"
SHIOAJI_GATEWAY_WS_URL: Final[str] = "ws://localhost:8000/ws"
