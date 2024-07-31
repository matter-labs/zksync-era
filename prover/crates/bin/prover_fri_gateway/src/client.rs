use std::sync::Arc;

use serde::{de::DeserializeOwned, Serialize};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};

/// A tiny wrapper over the reqwest client that also stores
/// the objects commonly needed when interacting with prover API.
#[derive(Debug)]
pub(crate) struct ProverApiClient {
    pub(crate) blob_store: Arc<dyn ObjectStore>,
    pub(crate) pool: ConnectionPool<Prover>,
    pub(crate) api_url: String,
    pub(crate) client: reqwest::Client,
}

impl ProverApiClient {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Prover>,
        api_url: String,
    ) -> Self {
        Self {
            blob_store,
            pool,
            api_url,
            client: reqwest::Client::new(),
        }
    }

    pub(crate) async fn send_http_request<Req, Resp>(
        &self,
        request: Req,
        endpoint: &str,
    ) -> Result<Resp, reqwest::Error>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        tracing::info!("Sending request to {}", endpoint);

        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<Resp>()
            .await
    }
}
