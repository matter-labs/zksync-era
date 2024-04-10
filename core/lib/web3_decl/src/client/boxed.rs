use std::fmt;

use async_trait::async_trait;
use jsonrpsee::core::{
    client::{BatchResponse, ClientT, Error},
    params::BatchRequestBuilder,
    traits::ToRpcParams,
    JsonRawValue,
};
use serde::de::DeserializeOwned;

use super::TaggedClient;

#[derive(Debug)]
struct RawParams(Option<Box<JsonRawValue>>);

impl RawParams {
    fn new(params: impl ToRpcParams) -> Result<Self, serde_json::Error> {
        params.to_rpc_params().map(Self)
    }
}

impl ToRpcParams for RawParams {
    fn to_rpc_params(self) -> Result<Option<Box<JsonRawValue>>, serde_json::Error> {
        Ok(self.0)
    }
}

/// Object-safe version of [`ClientT`] + [`Clone`] + [`TaggedClient`].
///
/// The implementation is fairly straightforward: [`RawParams`] is used as a catch-all params type,
/// and `serde_json::Value` is used as a catch-all response type.
#[async_trait]
trait ObjectSafeClient: 'static + Send + Sync + fmt::Debug {
    fn clone_boxed(&self) -> Box<dyn ObjectSafeClient>;

    fn for_component_boxed(
        self: Box<Self>,
        component_name: &'static str,
    ) -> Box<dyn ObjectSafeClient>;

    async fn notification(&self, method: &str, params: RawParams) -> Result<(), Error>;

    async fn request(&self, method: &str, params: RawParams) -> Result<serde_json::Value, Error>;

    async fn batch_request<'a>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, serde_json::Value>, Error>;
}

#[async_trait]
impl<C> ObjectSafeClient for C
where
    C: 'static + Send + Sync + Clone + fmt::Debug + ClientT + TaggedClient,
{
    fn clone_boxed(&self) -> Box<dyn ObjectSafeClient> {
        Box::new(self.clone())
    }

    fn for_component_boxed(
        self: Box<Self>,
        component_name: &'static str,
    ) -> Box<dyn ObjectSafeClient> {
        Box::new(self.for_component(component_name))
    }

    async fn notification(&self, method: &str, params: RawParams) -> Result<(), Error> {
        <C as ClientT>::notification(self, method, params).await
    }

    async fn request(&self, method: &str, params: RawParams) -> Result<serde_json::Value, Error> {
        <C as ClientT>::request(self, method, params).await
    }

    async fn batch_request<'a>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, serde_json::Value>, Error> {
        <C as ClientT>::batch_request(self, batch).await
    }
}

/// Boxed version of an L2 client.
///
/// For now, it has two implementations:
///
/// - [`L2Client`](super::L2Client) is the main implementation based on HTTP JSON-RPC
/// - [`MockL2Client`](super::MockL2Client) is a mock implementation.
#[derive(Debug)]
pub struct BoxedL2Client(Box<dyn ObjectSafeClient>);

impl Clone for BoxedL2Client {
    fn clone(&self) -> Self {
        Self(self.0.clone_boxed())
    }
}

impl BoxedL2Client {
    /// Boxes the provided client.
    pub fn new<C>(client: C) -> Self
    where
        C: 'static + Send + Sync + Clone + fmt::Debug + ClientT + TaggedClient,
    {
        Self(Box::new(client))
    }

    /// Tags this client with a component label, which will be shown in logs etc.
    pub fn for_component(self, component_name: &'static str) -> Self {
        Self(self.0.for_component_boxed(component_name))
    }
}

#[async_trait]
impl ClientT for BoxedL2Client {
    async fn notification<Params>(&self, method: &str, params: Params) -> Result<(), Error>
    where
        Params: ToRpcParams + Send,
    {
        self.0
            .as_ref()
            .notification(method, RawParams::new(params)?)
            .await
    }

    async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, Error>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        let raw_response = self
            .0
            .as_ref()
            .request(method, RawParams::new(params)?)
            .await?;
        serde_json::from_value(raw_response).map_err(Error::ParseError)
    }

    async fn batch_request<'a, R>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, R>, Error>
    where
        R: DeserializeOwned + fmt::Debug + 'a,
    {
        let raw_responses = self.0.as_ref().batch_request(batch).await?;
        let mut successful_calls = 0;
        let mut failed_calls = 0;
        let mut responses = Vec::with_capacity(raw_responses.len());
        for raw_response in raw_responses {
            responses.push(match raw_response {
                Ok(json) => {
                    successful_calls += 1;
                    Ok(serde_json::from_value::<R>(json)?)
                }
                Err(err) => {
                    failed_calls += 1;
                    Err(err)
                }
            })
        }
        Ok(BatchResponse::new(
            successful_calls,
            responses,
            failed_calls,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{client::MockL2Client, namespaces::EthNamespaceClient};

    #[tokio::test]
    async fn boxing_mock_client() {
        let client = MockL2Client::new(|method, params| {
            assert_eq!(method, "eth_blockNumber");
            assert_eq!(params, serde_json::Value::Null);
            Ok(serde_json::json!("0x42"))
        });
        let client = BoxedL2Client::new(client);
        let block_number = client.get_block_number().await.unwrap();
        assert_eq!(block_number, 0x42.into());
    }
}
