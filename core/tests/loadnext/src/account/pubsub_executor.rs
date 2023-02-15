use std::time::{Duration, Instant};

use futures::SinkExt;
use once_cell::sync::OnceCell;
use tokio::sync::Semaphore;

use zksync::error::ClientError;
use zksync_web3_decl::{
    jsonrpsee::{
        core::client::{Subscription, SubscriptionClientT},
        rpc_params,
        ws_client::WsClientBuilder,
    },
    types::PubSubResult,
};

use super::AccountLifespan;
use crate::{
    command::SubscriptionType,
    report::{ReportBuilder, ReportLabel},
    rng::WeightedRandom,
};
use zksync::types::PubSubFilterBuilder;

/// Shared semaphore which limits the number of active subscriptions.
/// Lazily initialized by the first account accessing it.
static REQUEST_LIMITER: OnceCell<Semaphore> = OnceCell::new();

impl AccountLifespan {
    async fn start_single_subscription_task(
        &mut self,
        subscription_type: SubscriptionType,
    ) -> Result<(), ClientError> {
        let client = WsClientBuilder::default()
            .build(&self.config.l2_ws_rpc_address)
            .await?;
        let params = match subscription_type {
            SubscriptionType::Logs => {
                let topics = super::api_request_executor::random_topics(
                    &self.wallet.test_contract.contract,
                    &mut self.wallet.rng,
                );
                // Safety: `run_pubsub_task` checks whether the cell is initialized
                // at every loop iteration and skips logs action if it's not. Thus,
                // it's safe to unwrap it.
                let contract_address =
                    unsafe { self.wallet.deployed_contract_address.get_unchecked() };
                let filter = PubSubFilterBuilder::default()
                    .set_topics(Some(topics), None, None, None)
                    .set_address(vec![*contract_address])
                    .build();
                rpc_params![subscription_type.rpc_name(), filter]
            }
            _ => rpc_params![subscription_type.rpc_name()],
        };
        let mut subscription: Subscription<PubSubResult> = client
            .subscribe("eth_subscribe", params, "eth_unsubscribe")
            .await?;
        let start = Instant::now();
        let subscription_duration = Duration::from_secs(self.config.single_subscription_time_secs);
        loop {
            if let Ok(resp) = tokio::time::timeout(subscription_duration, subscription.next()).await
            {
                match resp {
                    None => return Err(ClientError::OperationTimeout),
                    Some(Err(err)) => return Err(err.into()),
                    _ => {}
                }
            }
            if start.elapsed() > subscription_duration {
                break;
            }
        }
        Ok(())
    }

    #[allow(clippy::let_underscore_future)]
    pub(super) async fn run_pubsub_task(self) {
        loop {
            let semaphore = REQUEST_LIMITER
                .get_or_init(|| Semaphore::new(self.config.sync_pubsub_subscriptions_limit));
            // The number of simultaneous subscriptions is limited by semaphore.
            let permit = semaphore
                .acquire()
                .await
                .expect("static semaphore cannot be closed");
            let mut self_ = self.clone();
            let _ = tokio::spawn(async move {
                let subscription_type = SubscriptionType::random(&mut self_.wallet.rng);
                let start = Instant::now();

                // Skip the action if the contract is not yet initialized for the account.
                let label = if let (SubscriptionType::Logs, None) = (
                    subscription_type,
                    self_.wallet.deployed_contract_address.get(),
                ) {
                    ReportLabel::skipped("Contract not deployed yet")
                } else {
                    let result = self_
                        .start_single_subscription_task(subscription_type)
                        .await;
                    match result {
                        Ok(_) => ReportLabel::ActionDone,
                        Err(err) => {
                            let error = err.to_string();
                            // Subscriptions can fail for a variety of reasons - no need to escalate it.
                            vlog::warn!(
                                "Subscription failed: {:?}, reason: {}",
                                subscription_type,
                                error
                            );
                            ReportLabel::ActionFailed { error }
                        }
                    }
                };
                let report = ReportBuilder::default()
                    .action(subscription_type)
                    .label(label)
                    .time(start.elapsed())
                    .reporter(self_.wallet.wallet.address())
                    .finish();
                drop(permit);
                let _ = self_.report_sink.send(report).await;
            });
        }
    }
}
