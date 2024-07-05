//! Mock L2 client implementation.

use std::{any, collections::HashMap, fmt, future::Future, marker::PhantomData, sync::Arc};

use async_trait::async_trait;
use futures::future;
use jsonrpsee::{
    core::{
        client::{BatchResponse, ClientT, Error},
        params::BatchRequestBuilder,
        traits::ToRpcParams,
    },
    types::{error::ErrorCode, ErrorObject},
};
use serde::{de::DeserializeOwned, Serialize};

use super::{boxed::RawParams, ForNetwork, Network, TaggedClient};

/// Object-safe counterpart to [`Handler`]. We need it because async closures aren't available on stable Rust.
#[async_trait]
trait HandleGenericRequest: Send + Sync + fmt::Debug {
    async fn handle_generic_request(
        &self,
        req: serde_json::Value,
    ) -> Result<serde_json::Value, Error>;
}

/// The only implementation of [`HandleGenericRequest`].
struct RequestHandler<Req, H, const ASYNC: bool> {
    inner: H,
    _fn: PhantomData<fn(Req)>,
}

impl<Req, H, const ASYNC: bool> fmt::Debug for RequestHandler<Req, H, ASYNC>
where
    Req: 'static,
    H: Handler<Req, ASYNC>,
    H::Resp: 'static,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RequestHandler")
            .field("async", &ASYNC)
            .field("req", &any::type_name::<Req>())
            .field("resp", &any::type_name::<H::Resp>())
            .finish()
    }
}

#[async_trait]
impl<Req, H, const ASYNC: bool> HandleGenericRequest for RequestHandler<Req, H, ASYNC>
where
    Req: DeserializeOwned + Send + 'static,
    H: Handler<Req, ASYNC>,
    H::Resp: Serialize + 'static,
{
    async fn handle_generic_request(
        &self,
        req: serde_json::Value,
    ) -> Result<serde_json::Value, Error> {
        let req: Req = serde_json::from_value(req)?;
        let resp = self.inner.call(req).await?;
        Ok(serde_json::to_value(resp)?)
    }
}

/// Request handler for [`MockClient`]. Implemented automatically for sync and async closures
/// taking zero to 3 de-serializable args (you might want to specify their types explicitly)
/// and returning `Result<Resp, Error>`, where `Resp` is serializable.
pub trait Handler<Req, const ASYNC: bool>: Send + Sync + 'static {
    /// Successful response.
    type Resp: Serialize + 'static;
    /// Handler future.
    type Fut: Future<Output = Result<Self::Resp, Error>> + Send + 'static;
    /// Calls the handler.
    fn call(&self, req: Req) -> Self::Fut;
}

macro_rules! impl_handler_for_tuple {
    ($($field:tt : $typ:ident),*) => {
        impl<F, $($typ,)* Resp> Handler<($($typ,)*), true> for F
        where
            F: Fn($($typ,)*) -> Result<Resp, Error> + Send + Sync + 'static,
            $($typ: DeserializeOwned + 'static,)*
            Resp: Serialize + Send + 'static,
        {
            type Resp = Resp;
            type Fut = future::Ready<Result<Resp, Error>>;

            #[allow(unused_variables)] // for `()`
            fn call(&self, req: ($($typ,)*)) -> Self::Fut {
                future::ready(self($(req.$field,)*))
            }
        }

        impl<F, Fut, $($typ,)* Resp> Handler<($($typ,)*), false> for F
        where
            F: Fn($($typ,)*) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = Result<Resp, Error>> + Send + 'static,
            $($typ: DeserializeOwned + 'static,)*
            Resp: Serialize + Send + 'static,
        {
            type Resp = Resp;
            type Fut = Fut;

            #[allow(unused_variables)] // for `()`
            fn call(&self, req: ($($typ,)*)) -> Self::Fut {
                self($(req.$field,)*)
            }
        }
    };
}

impl_handler_for_tuple!();
impl_handler_for_tuple!(0: A);
impl_handler_for_tuple!(0: A, 1: B);
impl_handler_for_tuple!(0: A, 1: B, 2: C);

/// Builder for [`MockClient`].
#[derive(Debug)]
pub struct MockClientBuilder<Net> {
    request_handlers: HashMap<&'static str, Box<dyn HandleGenericRequest>>,
    network: Net,
}

impl<Net: Network> MockClientBuilder<Net> {
    /// Adds a mock method handler to this builder.
    #[must_use]
    pub fn method<Req, const ASYNC: bool>(
        mut self,
        method: &'static str,
        request_handler: impl Handler<Req, ASYNC>,
    ) -> Self
    where
        Req: DeserializeOwned + Send + 'static,
    {
        let handler = RequestHandler {
            inner: request_handler,
            _fn: PhantomData,
        };
        self.request_handlers.insert(method, Box::new(handler));
        self
    }

    pub fn build(self) -> MockClient<Net> {
        MockClient {
            request_handlers: Arc::new(self.request_handlers),
            component_name: "",
            network: self.network,
        }
    }
}

/// Mock L2 client implementation. For now, it only mocks requests and batch requests; all other
/// interactions with the client will panic.
#[derive(Clone)]
pub struct MockClient<Net> {
    request_handlers: Arc<HashMap<&'static str, Box<dyn HandleGenericRequest>>>,
    component_name: &'static str,
    network: Net,
}

impl<Net> fmt::Debug for MockClient<Net> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MockL2Client")
            .finish_non_exhaustive()
    }
}

impl<Net: Network> MockClient<Net> {
    pub fn builder(network: Net) -> MockClientBuilder<Net> {
        MockClientBuilder {
            request_handlers: HashMap::new(),
            network,
        }
    }
}

impl<Net: Network> ForNetwork for MockClient<Net> {
    type Net = Net;

    fn network(&self) -> Self::Net {
        self.network
    }

    fn component(&self) -> &'static str {
        self.component_name
    }
}

impl<Net: Network> TaggedClient for MockClient<Net> {
    fn set_component(&mut self, component_name: &'static str) {
        self.component_name = component_name;
    }
}

#[async_trait]
impl<Net: Network> ClientT for MockClient<Net> {
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
        let handler = self.request_handlers.get(method).ok_or_else(|| {
            Error::Call(ErrorObject::owned(
                ErrorCode::MethodNotFound.code(),
                ErrorCode::MethodNotFound.message(),
                None::<()>,
            ))
        })?;
        let raw_response = handler.handle_generic_request(params).await?;
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
            .map(|(method, value)| self.request(method, RawParams(value)));
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
