from typing import Any

from nautilus_trader.common.providers import InstrumentProvider
from nautilus_trader.config import InstrumentProviderConfig
from nautilus_trader.core import nautilus_pyo3
from nautilus_trader.core.correctness import PyCondition
from nautilus_trader.model.identifiers import InstrumentId
from nautilus_trader.model.instruments import instruments_from_pyo3


class ShioajiInstrumentProvider(InstrumentProvider):
    """
    Provides Nautilus instrument definitions from Shioaji (SinoPac) gateway.

    Loads stocks, futures, and options contracts via the Rust HTTP client
    and converts them to Nautilus instrument types.

    Parameters
    ----------
    client : nautilus_pyo3.shioaji.ShioajiHttpClient
        The Shioaji gateway HTTP client.
    config : InstrumentProviderConfig, optional
        The instrument provider configuration, by default None.

    """

    def __init__(
        self,
        client: nautilus_pyo3.shioaji.ShioajiHttpClient,
        config: InstrumentProviderConfig | None = None,
    ) -> None:
        super().__init__(config=config)
        self._client = client

        self._instruments_pyo3: list[Any] = []

    def instruments_pyo3(self) -> list[Any]:
        """
        Return the raw pyo3 instruments for passing to the Rust client.

        Returns
        -------
        list[nautilus_pyo3.Instrument]

        """
        return self._instruments_pyo3

    async def load_all_async(self, filters: dict | None = None) -> None:
        """
        Load all instruments from the Shioaji gateway.

        Parameters
        ----------
        filters : dict, optional
            Not implemented for Shioaji (all contracts are loaded).

        """
        all_pyo3_instruments: list[Any] = []

        # Load stocks
        try:
            stocks = await self._client.request_stock_instruments()
            all_pyo3_instruments.extend(stocks)
            self._log.info(f"Loaded {len(stocks)} stock instruments")
        except Exception as e:
            self._log.error(f"Failed to load stocks: {e}")

        # Load futures
        try:
            futures = await self._client.request_futures_instruments()
            all_pyo3_instruments.extend(futures)
            self._log.info(f"Loaded {len(futures)} futures instruments")
        except Exception as e:
            self._log.error(f"Failed to load futures: {e}")

        # Load options
        try:
            options = await self._client.request_options_instruments()
            all_pyo3_instruments.extend(options)
            self._log.info(f"Loaded {len(options)} options instruments")
        except Exception as e:
            self._log.error(f"Failed to load options: {e}")

        self._instruments_pyo3 = all_pyo3_instruments

        # Convert pyo3 instruments to Python Nautilus instruments
        instruments = instruments_from_pyo3(all_pyo3_instruments)
        for instrument in instruments:
            self.add(instrument=instrument)

        self._log.info(
            f"Total instruments loaded: {len(instruments)} "
            f"({self.count} registered)",
        )

    async def load_ids_async(
        self,
        instrument_ids: list[InstrumentId],
        filters: dict | None = None,
    ) -> None:
        """
        Load specific instruments by ID from Shioaji.

        Parameters
        ----------
        instrument_ids : list[InstrumentId]
            The instrument IDs to load.
        filters : dict, optional
            Not implemented for Shioaji.

        """
        if not instrument_ids:
            self._log.warning("No instrument IDs given for loading")
            return

        # Shioaji doesn't support per-instrument queries, load all and filter
        all_pyo3_instruments: list[Any] = []

        try:
            stocks = await self._client.request_stock_instruments()
            all_pyo3_instruments.extend(stocks)
        except Exception as e:
            self._log.error(f"Failed to load stocks: {e}")

        try:
            futures = await self._client.request_futures_instruments()
            all_pyo3_instruments.extend(futures)
        except Exception as e:
            self._log.error(f"Failed to load futures: {e}")

        try:
            options = await self._client.request_options_instruments()
            all_pyo3_instruments.extend(options)
        except Exception as e:
            self._log.error(f"Failed to load options: {e}")

        self._instruments_pyo3 = all_pyo3_instruments

        instruments = instruments_from_pyo3(all_pyo3_instruments)
        for instrument in instruments:
            if instrument.id not in instrument_ids:
                continue
            self.add(instrument=instrument)

        self._log.info(f"Loaded {len(self._instruments)} instruments from Shioaji")

    async def load_async(
        self,
        instrument_id: InstrumentId,
        filters: dict | None = None,
    ) -> None:
        """
        Load a single instrument by ID from Shioaji.

        Parameters
        ----------
        instrument_id : InstrumentId
            The instrument ID to load.
        filters : dict, optional
            Not implemented for Shioaji.

        """
        PyCondition.not_none(instrument_id, "instrument_id")
        await self.load_ids_async([instrument_id], filters)
