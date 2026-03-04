use std::collections::HashMap;

use nautilus_network::http::{HttpClient, Method};
use serde::{Serialize, de::DeserializeOwned};

use super::error::ShioajiHttpError;
use crate::common::{consts::SHIOAJI_GATEWAY_HTTP_URL, urls::gateway_http_url};

/// HTTP client for communicating with the Shioaji FastAPI gateway.
#[derive(Clone, Debug)]
pub struct ShioajiHttpClient {
    base_url: String,
    client: HttpClient,
}

impl ShioajiHttpClient {
    /// Creates a new [`ShioajiHttpClient`].
    ///
    /// The `base_url` is the raw gateway URL (e.g. `http://localhost:8000`).
    /// The `/api` prefix is appended automatically via [`gateway_http_url`].
    pub fn new(base_url: Option<String>) -> Result<Self, ShioajiHttpError> {
        let raw_base = base_url.unwrap_or_else(|| SHIOAJI_GATEWAY_HTTP_URL.to_string());
        let base_url = gateway_http_url(&raw_base);
        let client = HttpClient::new(HashMap::new(), Vec::new(), Vec::new(), None, Some(30), None)?;
        Ok(Self { base_url, client })
    }

    /// Returns the base URL for the gateway.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Sends a GET request and deserializes the JSON response.
    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ShioajiHttpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .request(Method::GET, url, None, None, None, None, None)
            .await?;

        if response.status.as_u16() >= 400 {
            let body = String::from_utf8_lossy(&response.body).to_string();
            return Err(ShioajiHttpError::GatewayError {
                status: response.status.as_u16(),
                body,
            });
        }

        serde_json::from_slice(&response.body).map_err(ShioajiHttpError::from)
    }

    /// Sends a GET request with query parameters and deserializes the JSON response.
    async fn get_with_params<T: DeserializeOwned, P: Serialize>(
        &self,
        path: &str,
        params: &P,
    ) -> Result<T, ShioajiHttpError> {
        let url = format!("{}{path}", self.base_url);
        let response = self
            .client
            .request_with_params(Method::GET, url, Some(params), None, None, None, None)
            .await?;

        if response.status.as_u16() >= 400 {
            let body = String::from_utf8_lossy(&response.body).to_string();
            return Err(ShioajiHttpError::GatewayError {
                status: response.status.as_u16(),
                body,
            });
        }

        serde_json::from_slice(&response.body).map_err(ShioajiHttpError::from)
    }

    /// Sends a POST request with a JSON body and deserializes the JSON response.
    async fn post<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ShioajiHttpError> {
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
            return Err(ShioajiHttpError::GatewayError {
                status: response.status.as_u16(),
                body,
            });
        }

        serde_json::from_slice(&response.body).map_err(ShioajiHttpError::from)
    }

    /// Sends a PUT request with a JSON body and deserializes the JSON response.
    async fn put<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ShioajiHttpError> {
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
            return Err(ShioajiHttpError::GatewayError {
                status: response.status.as_u16(),
                body,
            });
        }

        serde_json::from_slice(&response.body).map_err(ShioajiHttpError::from)
    }

    /// Sends a DELETE request with a JSON body and deserializes the JSON response.
    async fn delete<T: DeserializeOwned, B: Serialize>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ShioajiHttpError> {
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
            return Err(ShioajiHttpError::GatewayError {
                status: response.status.as_u16(),
                body,
            });
        }

        serde_json::from_slice(&response.body).map_err(ShioajiHttpError::from)
    }
}
