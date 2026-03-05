from __future__ import annotations

import asyncio
import os

from nautilus_trader.adapters.shioaji.config import ShioajiExecClientConfig
from nautilus_trader.adapters.shioaji.constants import SINOPAC
from nautilus_trader.adapters.shioaji.constants import SINOPAC_VENUE
from nautilus_trader.adapters.shioaji.providers import ShioajiInstrumentProvider
from nautilus_trader.cache.cache import Cache
from nautilus_trader.common.component import LiveClock
from nautilus_trader.common.component import MessageBus
from nautilus_trader.common.enums import LogColor
from nautilus_trader.core import nautilus_pyo3
from nautilus_trader.core.nautilus_pyo3 import shioaji as pyo3_shioaji
from nautilus_trader.live.cancellation import DEFAULT_FUTURE_CANCELLATION_TIMEOUT
from nautilus_trader.live.cancellation import cancel_tasks_with_timeout
from nautilus_trader.live.execution_client import LiveExecutionClient
from nautilus_trader.model.currencies import Currency
from nautilus_trader.model.enums import AccountType
from nautilus_trader.model.enums import OmsType
from nautilus_trader.model.identifiers import AccountId
from nautilus_trader.model.identifiers import ClientId


class ShioajiExecutionClient(LiveExecutionClient):
    """
    Provides an execution client for the Shioaji (SinoPac) adapter.

    Parameters
    ----------
    loop : asyncio.AbstractEventLoop
        The event loop for the client.
    client : pyo3_shioaji.ShioajiHttpClient
        The Shioaji gateway HTTP client.
    ws_client : pyo3_shioaji.ShioajiWebSocketClient
        The Shioaji gateway WebSocket client.
    msgbus : MessageBus
        The message bus for the client.
    cache : Cache
        The cache for the client.
    clock : LiveClock
        The clock for the client.
    instrument_provider : ShioajiInstrumentProvider
        The instrument provider.
    config : ShioajiExecClientConfig
        The configuration for the client.
    name : str, optional
        The custom client ID.

    """

    def __init__(
        self,
        loop: asyncio.AbstractEventLoop,
        client: pyo3_shioaji.ShioajiHttpClient,
        ws_client: pyo3_shioaji.ShioajiWebSocketClient,
        msgbus: MessageBus,
        cache: Cache,
        clock: LiveClock,
        instrument_provider: ShioajiInstrumentProvider,
        config: ShioajiExecClientConfig,
        name: str | None = None,
    ) -> None:
        account_id_str = config.account_id or os.environ.get(
            "SHIOAJI_ACCOUNT_ID",
            "SINOPAC-001",
        )
        account_id = AccountId(f"{SINOPAC}-{account_id_str}")

        super().__init__(
            loop=loop,
            client_id=ClientId(name or SINOPAC),
            venue=SINOPAC_VENUE,
            oms_type=OmsType.NETTING,
            account_type=AccountType.CASH,
            base_currency=None,
            instrument_provider=instrument_provider,
            msgbus=msgbus,
            cache=cache,
            clock=clock,
            config=config,
        )

        self._http_client = client
        self._ws_client = ws_client
        self._config = config
        self._account_id = account_id
        self._client_futures: set[asyncio.Future] = set()

        # Maps trade_id (VenueOrderId) → client_order_id for WS event correlation
        self._trade_id_to_client_order_id: dict[str, str] = {}

    # -- Connection lifecycle -------------------------------------------------

    async def _connect(self) -> None:
        await self._instrument_provider.initialize()
        await self._update_account_state()
        self._log.info(
            f"Connected to Shioaji gateway at {self._config.gateway_base_url}",
            LogColor.GREEN,
        )

    async def _disconnect(self) -> None:
        await cancel_tasks_with_timeout(
            self._client_futures,
            self._log,
            timeout_secs=DEFAULT_FUTURE_CANCELLATION_TIMEOUT,
        )
        self._client_futures.clear()

    async def _update_account_state(self) -> None:
        """Query account balance and generate AccountState event."""
        try:
            balance_data = await self._http_client.account_balance()
            from nautilus_trader.model.objects import AccountBalance as NTAccountBalance
            from nautilus_trader.model.objects import Money

            twd = Currency.from_str("TWD")
            balances = [
                NTAccountBalance(
                    total=Money(balance_data["balance"], twd),
                    locked=Money(0, twd),
                    free=Money(balance_data["balance"], twd),
                ),
            ]
            self.generate_account_state(
                balances=balances,
                margins=[],
                reported=True,
                ts_event=self._clock.timestamp_ns(),
            )
        except Exception as e:
            self._log.error(f"Failed to update account state: {e}")

    # -- WS message handler ---------------------------------------------------

    def _handle_msg(self, msg) -> None:
        """Handle incoming messages from Shioaji WS (order events as dicts)."""
        try:
            if nautilus_pyo3.is_pycapsule(msg):
                return  # Market data — handled by DataClient

            if isinstance(msg, dict):
                self._handle_order_event(msg)
                return

            self._log.warning(f"Unhandled exec WS message type: {type(msg)}")
        except Exception as e:
            self._log.exception("Error handling Shioaji exec WS message", e)

    def _handle_order_event(self, event: dict) -> None:
        """Dispatch order event dict to appropriate handler."""
        event_type = event.get("event_type")
        if event_type in ("stock_order", "futures_order"):
            self._handle_order_status_event(event)
        elif event_type in ("stock_deal", "futures_deal"):
            self._handle_deal_event(event)
        else:
            self._log.warning(f"Unknown order event type: {event_type}")

    def _handle_order_status_event(self, event: dict) -> None:
        """Handle a stock/futures order status event from WS."""

    def _handle_deal_event(self, event: dict) -> None:
        """Handle a stock/futures deal (fill) event from WS."""
