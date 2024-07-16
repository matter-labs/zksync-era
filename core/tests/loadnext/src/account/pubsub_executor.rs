use std::time::{Duration, Instant};

use futures::{stream, TryStreamExt};
use zksync_web3_decl::{
    jsonrpsee::{
        core::{
            client::{Subscription, SubscriptionClientT},
            ClientError as RpcError,
        },
        rpc_params,
        ws_client::WsClientBuilder,
    },
    types::PubSubResult,
};

use super::{Aborted, AccountLifespan};
use crate::{
    command::SubscriptionType,
    config::RequestLimiters,
    report::{ReportBuilder, ReportLabel},
    rng::WeightedRandom,
    sdk::{error::ClientError, types::PubSubFilterBuilder},
};

impl AccountLifespan {
    async fn run_single_subscription(mut self, limiters: &RequestLimiters) -> Result<(), Aborted> {
        let permit = limiters.subscriptions.acquire().await.unwrap();
        let subscription_type = SubscriptionType::random(&mut self.wallet.rng);
        let start = Instant::now();

        // Skip the action if the contract is not yet initialized for the account.
        let label = if let (SubscriptionType::Logs, None) = (
            subscription_type,
            self.wallet.deployed_contract_address.get(),
        ) {
            ReportLabel::skipped("Contract not deployed yet")
        } else {
            let result = self.start_single_subscription_task(subscription_type).await;
            match result {
                Ok(_) => ReportLabel::ActionDone,
                Err(err) => {
                    // Subscriptions can fail for a variety of reasons - no need to escalate it.
                    tracing::warn!("Subscription failed: {subscription_type:?}, reason: {err}");
                    ReportLabel::failed(err.to_string())
                }
            }
        };
        drop(permit);

        let report = ReportBuilder::default()
            .action(subscription_type)
            .label(label)
            .time(start.elapsed())
            .reporter(self.wallet.wallet.address())
            .finish();
        self.send_report(report).await?;
        Ok(())
    }

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
                let contract_address = self.wallet.deployed_contract_address.get().unwrap();
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
                    Some(Err(err)) => return Err(RpcError::ParseError(err).into()),
                    _ => {}
                }
            }
            if start.elapsed() > subscription_duration {
                break;
            }
        }
        Ok(())
    }

    pub(super) async fn run_pubsub_task(self, limiters: &RequestLimiters) -> Result<(), Aborted> {
        // We use `try_for_each_concurrent` to propagate test abortion, but we cannot
        // rely solely on its concurrency limiter because we need to limit concurrency
        // for all accounts in total, rather than for each account separately.
        let local_limit = (self.config.sync_pubsub_subscriptions_limit / 5).max(1);
        let subscriptions = stream::repeat_with(move || Ok(self.clone()));
        subscriptions
            .try_for_each_concurrent(local_limit, |this| this.run_single_subscription(limiters))
            .await
    }
}
