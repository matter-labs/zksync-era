//! Mock L2 client implementation.

use std::{fmt, future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;
use futures::future;
use jsonrpsee::core::{
    client::{BatchResponse, ClientT, Error, Subscription, SubscriptionClientT},
    params::BatchRequestBuilder,
    traits::ToRpcParams,
};
use serde::de::DeserializeOwned;

use super::TaggedClient;

type MockHandleResult<'a> =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, Error>> + Send + 'a>>;
type RequestHandler = dyn Fn(&str, serde_json::Value) -> MockHandleResult<'_> + Send + Sync;

/// Mock L2 client implementation. For now, it only mocks requests and batch requests; all other
/// interactions with the client will panic.
#[derive(Clone)]
pub struct MockL2Client {
    request_handler: Arc<RequestHandler>,
    component_name: &'static str,
}

impl fmt::Debug for MockL2Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MockL2Client")
            .finish_non_exhaustive()
    }
}

impl MockL2Client {
    /// Creates an L2 client based on the provided request handler.
    pub fn new<F>(request_handler: F) -> Self
    where
        F: Fn(&str, serde_json::Value) -> Result<serde_json::Value, Error> + Send + Sync + 'static,
    {
        Self {
            request_handler: Arc::new(move |method, params| {
                Box::pin(future::ready(request_handler(method, params)))
            }),
            component_name: "",
        }
    }

    /// Creates an L2 client based on the provided async request handler.
    pub fn new_async<F>(request_handler: F) -> Self
    where
        F: Fn(&str, serde_json::Value) -> MockHandleResult<'_> + Send + Sync + 'static,
    {
        Self {
            request_handler: Arc::new(request_handler),
            component_name: "",
        }
    }
}

impl TaggedClient for MockL2Client {
    fn for_component(mut self, component_name: &'static str) -> Self {
        self.component_name = component_name;
        self
    }
}

#[async_trait]
impl ClientT for MockL2Client {
    async fn notification<Params>(&self, _method: &str, _params: Params) -> Result<(), Error>
    where
        Params: ToRpcParams + Send,
    {
        unimplemented!("never used in the codebase")
    }

    async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, Error>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        let params = params.to_rpc_params()?;
        let params: serde_json::Value = if let Some(raw_value) = params {
            serde_json::from_str(raw_value.get())?
        } else {
            serde_json::Value::Null
        };
        let raw_response = (self.request_handler)(method, params).await?;
        Ok(serde_json::from_value(raw_response)?)
    }

    async fn batch_request<'a, R>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, R>, Error>
    where
        R: DeserializeOwned + fmt::Debug + 'a,
    {
        let request_handlers = batch
            .into_iter()
            .map(|(method, _)| self.request::<serde_json::Value, _>(method, [()]));
        let response_results = future::join_all(request_handlers).await;
        let mut responses = vec![];
        let mut successful_calls = 0;
        let mut failed_calls = 0;
        for result in response_results {
            match result {
                Ok(value) => {
                    responses.push(Ok(serde_json::from_value(value)?));
                    successful_calls += 1;
                }
                Err(Error::Call(err)) => {
                    responses.push(Err(err));
                    failed_calls += 1;
                }
                Err(err) => return Err(err),
            }
        }
        Ok(BatchResponse::new(
            successful_calls,
            responses,
            failed_calls,
        ))
    }
}

#[async_trait]
impl SubscriptionClientT for MockL2Client {
    async fn subscribe<'a, Notif, Params>(
        &self,
        _subscribe_method: &'a str,
        _params: Params,
        _unsubscribe_method: &'a str,
    ) -> Result<Subscription<Notif>, Error>
    where
        Params: ToRpcParams + Send,
        Notif: DeserializeOwned,
    {
        unimplemented!("never used in the codebase")
    }

    async fn subscribe_to_method<'a, Notif>(
        &self,
        _method: &'a str,
    ) -> Result<Subscription<Notif>, Error>
    where
        Notif: DeserializeOwned,
    {
        unimplemented!("never used in the codebase")
    }
}
