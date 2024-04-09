use std::fmt;

use async_trait::async_trait;
use jsonrpsee::core::{
    client::{BatchResponse, ClientT, Error, Subscription, SubscriptionClientT},
    params::BatchRequestBuilder,
    traits::ToRpcParams,
    JsonRawValue,
};
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;
use tracing::Instrument;

#[derive(Debug)]
pub struct RawParams(Option<Box<JsonRawValue>>);

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

#[async_trait]
trait ObjectSafeClient: 'static + Send + Sync + fmt::Debug {
    fn clone_boxed(&self) -> Box<dyn ObjectSafeClient>;

    async fn notification(&self, method: &str, params: RawParams) -> Result<(), Error>;

    async fn request(&self, method: &str, params: RawParams) -> Result<serde_json::Value, Error>;

    async fn batch_request<'a>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, serde_json::Value>, Error>;

    async fn subscribe<'a>(
        &self,
        subscribe_method: &'a str,
        params: RawParams,
        unsubscribe_method: &'a str,
    ) -> Result<Subscription<serde_json::Value>, Error>;

    async fn subscribe_to_method<'a>(
        &self,
        method: &'a str,
    ) -> Result<Subscription<serde_json::Value>, Error>;
}

#[async_trait]
impl<C> ObjectSafeClient for C
where
    C: 'static + Send + Sync + Clone + fmt::Debug + SubscriptionClientT,
{
    fn clone_boxed(&self) -> Box<dyn ObjectSafeClient> {
        Box::new(self.clone())
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

    async fn subscribe<'a>(
        &self,
        subscribe_method: &'a str,
        params: RawParams,
        unsubscribe_method: &'a str,
    ) -> Result<Subscription<serde_json::Value>, Error> {
        <C as SubscriptionClientT>::subscribe(self, subscribe_method, params, unsubscribe_method)
            .await
    }

    async fn subscribe_to_method<'a>(
        &self,
        method: &'a str,
    ) -> Result<Subscription<serde_json::Value>, Error> {
        <C as SubscriptionClientT>::subscribe_to_method(self, method).await
    }
}

/// Boxed version of L2 client.
#[derive(Debug)]
pub struct BoxedL2Client(Box<dyn ObjectSafeClient>);

impl Clone for BoxedL2Client {
    fn clone(&self) -> Self {
        Self(self.0.clone_boxed())
    }
}

impl BoxedL2Client {
    pub fn new<C>(client: C) -> Self
    where
        C: 'static + Send + Sync + Clone + fmt::Debug + SubscriptionClientT,
    {
        Self(Box::new(client))
    }

    fn translate_subscription<N: DeserializeOwned>(
        mut raw: Subscription<serde_json::Value>,
    ) -> Subscription<N> {
        let kind = raw.kind().clone();
        let (command_sender, mut command_receiver) = mpsc::channel(1);
        let (notif_sender, notif_receiver) = mpsc::channel(1);
        let translation_task = async move {
            loop {
                tokio::select! {
                    Some(notif_result) = raw.next() => {
                        match notif_result {
                            Ok(json) => {
                                if notif_sender.send(json).await.is_err() {
                                    tracing::info!("Failed sending notification; exiting");
                                    break;
                                }
                            },
                            Err(err) => {
                                tracing::warn!("Deserializing `serde_json::Value` failed: {err}");
                                break;
                            }
                        }
                    }
                    Some(command) = command_receiver.recv() => {
                        tracing::debug!(?command, "Received unsubscribe command; exiting");
                        raw.unsubscribe().await.ok();
                        break;
                    }
                    else => {
                        // if either the command sender or the upstream notification is dropped, stop the loop immediately
                        tracing::debug!("Command sender or upstream notification are dropped; exiting");
                        break;
                    }
                }
            }
        };
        tokio::spawn(translation_task.instrument(tracing::info_span!("translation_task", ?kind)));
        Subscription::new(command_sender, notif_receiver, kind)
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

#[async_trait]
impl SubscriptionClientT for BoxedL2Client {
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
        let raw_subscription = self
            .0
            .as_ref()
            .subscribe(
                subscribe_method,
                RawParams::new(params)?,
                unsubscribe_method,
            )
            .await?;
        Ok(Self::translate_subscription(raw_subscription))
    }

    async fn subscribe_to_method<'a, Notif>(
        &self,
        method: &'a str,
    ) -> Result<Subscription<Notif>, Error>
    where
        Notif: DeserializeOwned,
    {
        let raw_subscription = self.0.as_ref().subscribe_to_method(method).await?;
        Ok(Self::translate_subscription(raw_subscription))
    }
}
