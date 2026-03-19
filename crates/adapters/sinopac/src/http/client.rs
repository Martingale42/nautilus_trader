use std::collections::HashMap;

use nautilus_network::http::{HttpClient, Method};
use serde::{Serialize, de::DeserializeOwned};

use super::{
    error::SinopacHttpError,
    models::*,
    query::{KBarsQuery, PositionsQuery, SnapshotsQuery, TicksQuery},
};
use crate::common::{consts::SINOPAC_GATEWAY_HTTP_URL, urls::gateway_http_url};

/// HTTP client for communicating with the Sinopac FastAPI gateway.
#[derive(Clone, Debug)]
#[cfg_attr(
    feature = "python",
    pyo3::pyclass(module = "nautilus_pyo3.sinopac", skip_from_py_object)
)]
pub struct SinopacHttpClient {
    base_url: String,
    client: HttpClient,
}

impl SinopacHttpClient {
    /// Creates a new [`SinopacHttpClient`].
    ///
    /// The `base_url` is the raw gateway URL (e.g. `http://localhost:8000`).
    /// The `/api` prefix is appended automatically via [`gateway_http_url`].
    pub fn new(base_url: Option<String>) -> Result<Self, SinopacHttpError> {
        let raw_base = base_url.unwrap_or_else(|| SINOPAC_GATEWAY_HTTP_URL.to_string());
        let base_url = gateway_http_url(&raw_base);
        let client = HttpClient::new(HashMap::new(), Vec::new(), Vec::new(), None, Some(30), None)?;
        Ok(Self { base_url, client })
    }

    /// Returns the base URL for the gateway.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Sends a GET request and deserializes the JSON response.
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, SinopacHttpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .request(Method::GET, url, None, None, None, None, None)
            .await?;

        if response.status.as_u16() >= 400 {
            let body = String::from_utf8_lossy(&response.body).to_string();
            return Err(SinopacHttpError::GatewayError {
                status: response.status.as_u16(),
                body,
            });
        }

        serde_json::from_slice(&response.body).map_err(SinopacHttpError::from)
    }

    /// Sends a GET request with query parameters and deserializes the JSON response.
    async fn get_with_params<T: DeserializeOwned, P: Serialize>(
        &self,
        path: &str,
        params: &P,
    ) -> Result<T, SinopacHttpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .request_with_params(Method::GET, url, Some(params), None, None, None, None)
            .await?;

        if response.status.as_u16() >= 400 {
            let body = String::from_utf8_lossy(&response.body).to_string();
            return Err(SinopacHttpError::GatewayError {
                status: response.status.as_u16(),
                body,
            });
        }

        serde_json::from_slice(&response.body).map_err(SinopacHttpError::from)
    }

    /// Sends a POST request with a JSON body and deserializes the JSON response.
    async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, SinopacHttpError> {
        let url = format!("{}{path}", self.base_url);
        let body_bytes = serde_json::to_vec(body)?;
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json".to_string(),
        );
        let response = self
            .client
            .request(
                Method::POST,
                url,
                None,
                Some(headers),
                Some(body_bytes),
                None,
                None,
            )
            .await?;

        if response.status.as_u16() >= 400 {
            let body = String::from_utf8_lossy(&response.body).to_string();
            return Err(SinopacHttpError::GatewayError {
                status: response.status.as_u16(),
                body,
            });
        }

        serde_json::from_slice(&response.body).map_err(SinopacHttpError::from)
    }

    /// Sends a PUT request with a JSON body and deserializes the JSON response.
    async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, SinopacHttpError> {
        let url = format!("{}{path}", self.base_url);
        let body_bytes = serde_json::to_vec(body)?;
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json".to_string(),
        );
        let response = self
            .client
            .request(
                Method::PUT,
                url,
                None,
                Some(headers),
                Some(body_bytes),
                None,
                None,
            )
            .await?;

        if response.status.as_u16() >= 400 {
            let body = String::from_utf8_lossy(&response.body).to_string();
            return Err(SinopacHttpError::GatewayError {
                status: response.status.as_u16(),
                body,
            });
        }

        serde_json::from_slice(&response.body).map_err(SinopacHttpError::from)
    }

    /// Sends a DELETE request with a JSON body and deserializes the JSON response.
    async fn delete<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, SinopacHttpError> {
        let url = format!("{}{path}", self.base_url);
        let body_bytes = serde_json::to_vec(body)?;
        let mut headers = HashMap::new();
        headers.insert(
            "Content-Type".to_string(),
            "application/json".to_string(),
        );
        let response = self
            .client
            .request(
                Method::DELETE,
                url,
                None,
                Some(headers),
                Some(body_bytes),
                None,
                None,
            )
            .await?;

        if response.status.as_u16() >= 400 {
            let body = String::from_utf8_lossy(&response.body).to_string();
            return Err(SinopacHttpError::GatewayError {
                status: response.status.as_u16(),
                body,
            });
        }

        serde_json::from_slice(&response.body).map_err(SinopacHttpError::from)
    }

    // ─── Auth ────────────────────────────────────

    pub async fn login(&self, request: &LoginRequest) -> Result<LoginResponse, SinopacHttpError> {
        self.post("/auth/login", request).await
    }

    pub async fn logout(&self) -> Result<MessageResponse, SinopacHttpError> {
        self.post("/auth/logout", &serde_json::Value::Object(Default::default()))
            .await
    }

    pub async fn status(&self) -> Result<StatusResponse, SinopacHttpError> {
        self.get("/auth/status").await
    }

    // ─── Contracts ───────────────────────────────

    pub async fn list_stocks(&self) -> Result<Vec<StockContract>, SinopacHttpError> {
        self.get("/contracts/stocks").await
    }

    pub async fn get_stock(&self, code: &str) -> Result<StockContract, SinopacHttpError> {
        self.get(&format!("/contracts/stocks/{code}")).await
    }

    pub async fn list_futures(&self) -> Result<Vec<FuturesContract>, SinopacHttpError> {
        self.get("/contracts/futures").await
    }

    pub async fn list_options(&self) -> Result<Vec<OptionsContract>, SinopacHttpError> {
        self.get("/contracts/options").await
    }

    // ─── Market Data ─────────────────────────────

    pub async fn snapshots(
        &self,
        query: &SnapshotsQuery,
    ) -> Result<Vec<SnapshotData>, SinopacHttpError> {
        self.get_with_params("/market/snapshots", query).await
    }

    pub async fn ticks(&self, query: &TicksQuery) -> Result<TicksResponse, SinopacHttpError> {
        self.get_with_params("/market/ticks", query).await
    }

    pub async fn kbars(&self, query: &KBarsQuery) -> Result<KBarsResponse, SinopacHttpError> {
        self.get_with_params("/market/kbars", query).await
    }

    // ─── Orders ──────────────────────────────────

    pub async fn place_order(
        &self,
        request: &PlaceOrderRequest,
    ) -> Result<PlaceOrderResponse, SinopacHttpError> {
        self.post("/orders/place", request).await
    }

    pub async fn update_order(
        &self,
        request: &UpdateOrderRequest,
    ) -> Result<TradeIdResponse, SinopacHttpError> {
        self.put("/orders/update", request).await
    }

    pub async fn cancel_order(
        &self,
        request: &CancelOrderRequest,
    ) -> Result<TradeIdResponse, SinopacHttpError> {
        self.delete("/orders/cancel", request).await
    }

    pub async fn list_trades(&self) -> Result<Vec<TradeInfo>, SinopacHttpError> {
        self.get("/orders/trades").await
    }

    // ─── Account ─────────────────────────────────

    pub async fn list_positions(
        &self,
        query: &PositionsQuery,
    ) -> Result<Vec<Position>, SinopacHttpError> {
        self.get_with_params("/account/positions", query).await
    }

    pub async fn account_balance(&self) -> Result<AccountBalance, SinopacHttpError> {
        self.get("/account/balance").await
    }

    pub async fn margin(&self) -> Result<MarginInfo, SinopacHttpError> {
        self.get("/account/margin").await
    }

    pub async fn list_profit_loss(&self) -> Result<Vec<ProfitLoss>, SinopacHttpError> {
        self.get("/account/pnl").await
    }
}
