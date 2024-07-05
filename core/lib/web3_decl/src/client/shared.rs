//! `Shared` RPC client.

use std::{fmt, sync::Arc};

use async_trait::async_trait;
use jsonrpsee::core::{
    client::{BatchResponse, ClientT, Error, Subscription, SubscriptionClientT},
    params::BatchRequestBuilder,
    traits::ToRpcParams,
};
use serde::de::DeserializeOwned;

#[derive(Debug)]
pub struct Shared<T>(Arc<T>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: ClientT> Shared<T> {
    pub fn new(client: T) -> Self {
        Self(Arc::new(client))
    }
}

impl<T: ClientT> From<Arc<T>> for Shared<T> {
    fn from(client_arc: Arc<T>) -> Self {
        Self(client_arc)
    }
}

#[async_trait]
impl<T: ClientT + Send + Sync> ClientT for Shared<T> {
    async fn notification<Params>(&self, method: &str, params: Params) -> Result<(), Error>
    where
        Params: ToRpcParams + Send,
    {
        self.0.as_ref().notification(method, params).await
    }

    async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, Error>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        self.0.as_ref().request(method, params).await
    }

    async fn batch_request<'a, R>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, R>, Error>
    where
        R: DeserializeOwned + fmt::Debug + 'a,
    {
        self.0.batch_request(batch).await
    }
}

#[async_trait]
impl<T: SubscriptionClientT + Send + Sync> SubscriptionClientT for Shared<T> {
    async fn subscribe<'a, Notif, Params>(
        &self,
        subscribe_method: &'a str,
        params: Params,
        unsubscribe_method: &'a str,
    ) -> Result<Subscription<Notif>, Error>
    where
        Params: ToRpcParams + Send,
        Notif: DeserializeOwned,
    {
        self.0
            .as_ref()
            .subscribe(subscribe_method, params, unsubscribe_method)
            .await
    }

    async fn subscribe_to_method<'a, Notif>(
        &self,
        method: &'a str,
    ) -> Result<Subscription<Notif>, Error>
    where
        Notif: DeserializeOwned,
    {
        self.0.as_ref().subscribe_to_method(method).await
    }
}
