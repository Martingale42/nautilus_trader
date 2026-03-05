from nautilus_trader.config import LiveDataClientConfig
from nautilus_trader.config import LiveExecClientConfig


class ShioajiDataClientConfig(LiveDataClientConfig, frozen=True):
    """
    Configuration for ``ShioajiDataClient`` instances.

    Parameters
    ----------
    venue : str, default "SINOPAC"
        The venue for the client.
    gateway_host : str, default "localhost"
        The Shioaji gateway host address.
    gateway_port : int, default 8000
        The Shioaji gateway HTTP/WS port.
    gateway_ws_path : str, default "/ws"
        The WebSocket endpoint path on the gateway.

    """

    venue: str = "SINOPAC"
    gateway_host: str = "localhost"
    gateway_port: int = 8000
    gateway_ws_path: str = "/ws"

    @property
    def gateway_base_url(self) -> str:
        return f"http://{self.gateway_host}:{self.gateway_port}"

    @property
    def gateway_ws_url(self) -> str:
        return f"ws://{self.gateway_host}:{self.gateway_port}{self.gateway_ws_path}"


class ShioajiExecClientConfig(LiveExecClientConfig, frozen=True):
    """
    Configuration for ``ShioajiExecutionClient`` instances.

    Parameters
    ----------
    venue : str, default "SINOPAC"
        The venue for the client.
    account_id : str, optional
        The Shioaji account identifier. If None, sourced from SHIOAJI_ACCOUNT_ID env var.
    gateway_host : str, default "localhost"
        The Shioaji gateway host address.
    gateway_port : int, default 8000
        The Shioaji gateway HTTP/WS port.
    gateway_ws_path : str, default "/ws"
        The WebSocket endpoint path on the gateway.

    """

    venue: str = "SINOPAC"
    account_id: str | None = None
    gateway_host: str = "localhost"
    gateway_port: int = 8000
    gateway_ws_path: str = "/ws"

    @property
    def gateway_base_url(self) -> str:
        return f"http://{self.gateway_host}:{self.gateway_port}"

    @property
    def gateway_ws_url(self) -> str:
        return f"ws://{self.gateway_host}:{self.gateway_port}{self.gateway_ws_path}"
