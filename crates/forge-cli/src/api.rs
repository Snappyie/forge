//! HTTP client for the Forge API.

use serde_json::Value;

use crate::context::Context;
use crate::error::{from_status, CliError, Result};

/// A thin wrapper over the API, carrying the active context's credentials.
#[derive(Debug, Clone)]
pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
    access_token: Option<String>,
}

impl ApiClient {
    pub fn new(context: &Context) -> Result<Self> {
        let http = reqwest::Client::builder()
            // Fail rather than hang: a CLI should not block on a dead server.
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| CliError::Network(format!("could not build an HTTP client: {e}")))?;

        Ok(Self {
            http,
            base_url: context.api_url.clone(),
            access_token: context.access_token.clone(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Performs a request and returns the `data` payload.
    ///
    /// A non-2xx response becomes the error kind that matches the documented
    /// exit code, rather than a generic failure.
    pub async fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let mut request = self.http.request(method, &url);

        if let Some(token) = &self.access_token {
            request = request.bearer_auth(token);
        }
        if let Some(key) = idempotency_key {
            request = request.header("idempotency-key", key);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }

        let response = request
            .send()
            .await
            .map_err(|e| CliError::Network(format!("could not reach {url}: {e}")))?;

        let status = response.status();
        let raw = response
            .text()
            .await
            .map_err(|e| CliError::Network(format!("could not read the response: {e}")))?;

        let parsed: Value =
            serde_json::from_str(&raw).unwrap_or_else(|_| Value::String(raw.clone()));

        if !status.is_success() {
            return Err(from_status(status.as_u16(), &parsed));
        }

        // The API wraps successful responses in an envelope; return its `data`
        // so callers never have to unwrap it themselves.
        Ok(match parsed {
            Value::Object(ref map) => map
                .get("data")
                .cloned()
                // Endpoints that answer with a bare value (metrics) have no
                // envelope, so pass the body through unchanged.
                .unwrap_or(parsed),
            other => other,
        })
    }

    pub async fn get(&self, path: &str) -> Result<Value> {
        self.request(reqwest::Method::GET, path, None, None).await
    }

    pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
        self.request(reqwest::Method::POST, path, Some(body), None)
            .await
    }

    pub async fn post_idempotent(&self, path: &str, body: Value, key: &str) -> Result<Value> {
        self.request(reqwest::Method::POST, path, Some(body), Some(key))
            .await
    }

    pub async fn patch(&self, path: &str, body: Value) -> Result<Value> {
        self.request(reqwest::Method::PATCH, path, Some(body), None)
            .await
    }

    pub async fn delete(&self, path: &str) -> Result<Value> {
        self.request(reqwest::Method::DELETE, path, None, None)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_reports_its_base_url() {
        let context = Context::new("dev", "http://localhost:3000/api/v1");
        let client = ApiClient::new(&context).unwrap();
        assert_eq!(client.base_url(), "http://localhost:3000/api/v1");
    }

    #[test]
    fn the_access_token_is_carried_when_present() {
        let mut context = Context::new("dev", "http://localhost:3000/api/v1");
        context.access_token = Some("token-abc".into());
        let client = ApiClient::new(&context).unwrap();
        assert_eq!(client.access_token.as_deref(), Some("token-abc"));
    }

    #[test]
    fn an_anonymous_client_is_allowed() {
        // Health probes and login need no credential.
        let context = Context::new("dev", "http://localhost:3000/api/v1");
        let client = ApiClient::new(&context).unwrap();
        assert!(client.access_token.is_none());
    }
}
