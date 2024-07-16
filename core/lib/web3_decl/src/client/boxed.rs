use std::fmt;

use async_trait::async_trait;
use jsonrpsee::core::{
    client::{BatchResponse, ClientT, Error},
    params::BatchRequestBuilder,
    traits::ToRpcParams,
    JsonRawValue,
};
use serde::de::DeserializeOwned;

use super::{ForNetwork, Network, TaggedClient};

#[derive(Debug)]
pub struct RawParams(pub(super) Option<Box<JsonRawValue>>);

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

/// Object-safe version of [`ClientT`] + [`Clone`]. Should generally be used via [`DynClient`] type alias.
// The implementation is fairly straightforward: [`RawParams`] is used as a catch-all params type,
// and `serde_json::Value` is used as a catch-all response type.
#[async_trait]
pub trait ObjectSafeClient: 'static + Send + Sync + fmt::Debug + ForNetwork {
    /// Tags this client as working for a specific component. The component name can be used in logging,
    /// metrics etc.
    fn for_component(self: Box<Self>, component_name: &'static str) -> Box<DynClient<Self::Net>>;

    #[doc(hidden)] // implementation detail
    fn clone_boxed(&self) -> Box<DynClient<Self::Net>>;

    #[doc(hidden)] // implementation detail
    async fn generic_notification(&self, method: &str, params: RawParams) -> Result<(), Error>;

    #[doc(hidden)] // implementation detail
    async fn generic_request(
        &self,
        method: &str,
        params: RawParams,
    ) -> Result<serde_json::Value, Error>;

    #[doc(hidden)] // implementation detail
    async fn generic_batch_request<'a>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, serde_json::Value>, Error>;
}

/// Dynamically typed RPC client for a certain [`Network`].
pub type DynClient<Net> = dyn ObjectSafeClient<Net = Net>;

#[async_trait]
impl<C> ObjectSafeClient for C
where
    C: 'static + Send + Sync + Clone + fmt::Debug + ClientT + TaggedClient,
{
    fn clone_boxed(&self) -> Box<DynClient<C::Net>> {
        Box::new(<C as Clone>::clone(self))
    }

    fn for_component(mut self: Box<Self>, component_name: &'static str) -> Box<DynClient<C::Net>> {
        self.set_component(component_name);
        self
    }

    async fn generic_notification(&self, method: &str, params: RawParams) -> Result<(), Error> {
        <C as ClientT>::notification(self, method, params).await
    }

    async fn generic_request(
        &self,
        method: &str,
        params: RawParams,
    ) -> Result<serde_json::Value, Error> {
        <C as ClientT>::request(self, method, params).await
    }

    async fn generic_batch_request<'a>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, serde_json::Value>, Error> {
        <C as ClientT>::batch_request(self, batch).await
    }
}

impl<Net: Network> Clone for Box<DynClient<Net>> {
    fn clone(&self) -> Self {
        self.as_ref().clone_boxed()
    }
}

#[async_trait]
impl<Net: Network> ClientT for &DynClient<Net> {
    async fn notification<Params>(&self, method: &str, params: Params) -> Result<(), Error>
    where
        Params: ToRpcParams + Send,
    {
        (**self)
            .generic_notification(method, RawParams::new(params)?)
            .await
    }

    async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, Error>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        let raw_response = (**self)
            .generic_request(method, RawParams::new(params)?)
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
        let raw_responses = (**self).generic_batch_request(batch).await?;
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

// Delegates to the above `&DynClient<Net>` implementation.
#[async_trait]
impl<Net: Network> ClientT for Box<DynClient<Net>> {
    async fn notification<Params>(&self, method: &str, params: Params) -> Result<(), Error>
    where
        Params: ToRpcParams + Send,
    {
        ClientT::notification(&self.as_ref(), method, params).await
    }

    async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, Error>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        ClientT::request(&self.as_ref(), method, params).await
    }

    async fn batch_request<'a, R>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, R>, Error>
    where
        R: DeserializeOwned + fmt::Debug + 'a,
    {
        ClientT::batch_request(&self.as_ref(), batch).await
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::U64;

    use super::*;
    use crate::{
        client::{MockClient, L2},
        namespaces::EthNamespaceClient,
    };

    #[tokio::test]
    async fn boxing_mock_client() {
        let client = MockClient::builder(L2::default())
            .method("eth_blockNumber", || Ok(U64::from(0x42)))
            .build();
        let client = Box::new(client) as Box<DynClient<L2>>;

        let block_number = client.get_block_number().await.unwrap();
        assert_eq!(block_number, 0x42.into());
        let block_number = client.as_ref().get_block_number().await.unwrap();
        assert_eq!(block_number, 0x42.into());
    }

    #[tokio::test]
    async fn client_can_be_cloned() {
        let client = MockClient::builder(L2::default())
            .method("eth_blockNumber", || Ok(U64::from(0x42)))
            .build();
        let client = Box::new(client) as Box<DynClient<L2>>;

        let cloned_client = client.clone();
        let block_number = cloned_client.get_block_number().await.unwrap();
        assert_eq!(block_number, 0x42.into());

        let client_with_label = client.for_component("test");
        assert_eq!(client_with_label.component(), "test");
        let block_number = client_with_label.get_block_number().await.unwrap();
        assert_eq!(block_number, 0x42.into());
    }
}
