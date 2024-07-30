use std::{cmp::max, fmt::Debug, sync::Arc, time::Instant};

use anyhow::Context as _;
use tokio::{sync::watch, time::sleep};
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::{BoundEthInterface, Options};
use zksync_external_price_api::PriceAPIClient;
use zksync_node_fee_model::l1_gas_price::TxParamsProvider;
use zksync_types::{
    base_token_ratio::BaseTokenAPIRatio,
    ethabi::{Contract, Token},
    web3::{contract::Tokenize, BlockNumber},
    Address, U256,
};

use crate::metrics::{OperationResult, OperationResultLabels, METRICS};

#[derive(Debug, Clone)]
pub struct BaseTokenRatioPersisterL1Params {
    pub eth_client: Box<dyn BoundEthInterface>,
    pub gas_adjuster: Arc<dyn TxParamsProvider>,
    pub token_multiplier_setter_account_address: Address,
    pub chain_admin_contract: Contract,
    pub diamond_proxy_contract_address: Address,
    pub chain_admin_contract_address: Option<Address>,
}

#[derive(Debug, Clone)]
pub struct BaseTokenRatioPersister {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
    base_token_address: Address,
    price_api_client: Arc<dyn PriceAPIClient>,
    l1_params: Option<BaseTokenRatioPersisterL1Params>,
}

impl BaseTokenRatioPersister {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: ConnectionPool<Core>,
        config: BaseTokenAdjusterConfig,
        base_token_address: Address,
        price_api_client: Arc<dyn PriceAPIClient>,
        l1_params: Option<BaseTokenRatioPersisterL1Params>,
    ) -> Self {
        Self {
            pool,
            config,
            base_token_address,
            price_api_client,
            l1_params,
        }
    }

    /// Main loop for the base token ratio persister.
    /// Orchestrates fetching a new ratio, persisting it, and conditionally updating the L1 with it.
    pub async fn run(&mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(self.config.price_polling_interval());

        while !*stop_receiver.borrow_and_update() {
            if let Err(err) = self.loop_iteration().await {
                tracing::warn!(
                    "Error in the base_token_ratio_persister loop interaction {}",
                    err
                );
                if self.config.halt_on_error {
                    return Err(err)
                        .context("Failed to execute a base_token_ratio_persister loop iteration");
                }
            }

            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }
        }

        tracing::info!("Stop signal received, base_token_ratio_persister is shutting down");
        Ok(())
    }

    async fn loop_iteration(&self) -> anyhow::Result<()> {
        // TODO(PE-148): Consider shifting retry upon adding external API redundancy.
        let new_ratio = self.retry_fetch_ratio().await?;
        self.persist_ratio(new_ratio).await?;
        self.retry_update_ratio_on_l1(new_ratio).await
    }

    fn get_eth_fees(
        &self,
        l1_params: &BaseTokenRatioPersisterL1Params,
        prev_base_fee_per_gas: Option<u64>,
        prev_priority_fee_per_gas: Option<u64>,
    ) -> (u64, u64) {
        // Use get_blob_tx_base_fee here instead of get_base_fee to optimise for fast inclusion.
        // get_base_fee might cause the transaction to be stuck in the mempool for 10+ minutes.
        let mut base_fee_per_gas = l1_params.gas_adjuster.as_ref().get_blob_tx_base_fee();
        let mut priority_fee_per_gas = l1_params.gas_adjuster.as_ref().get_priority_fee();
        if let Some(x) = prev_priority_fee_per_gas {
            // Increase `priority_fee_per_gas` by at least 20% to prevent "replacement transaction under-priced" error.
            priority_fee_per_gas = max(priority_fee_per_gas, (x * 6) / 5 + 1);
        }

        if let Some(x) = prev_base_fee_per_gas {
            // same for base_fee_per_gas but 10%
            base_fee_per_gas = max(base_fee_per_gas, x + (x / 10) + 1);
        }

        // Extra check to prevent sending transaction will extremely high priority fee.
        if priority_fee_per_gas > self.config.max_acceptable_priority_fee_in_gwei {
            panic!(
                "Extremely high value of priority_fee_per_gas is suggested: {}, while max acceptable is {}",
                priority_fee_per_gas,
                self.config.max_acceptable_priority_fee_in_gwei
            );
        }

        (base_fee_per_gas, priority_fee_per_gas)
    }

    async fn retry_update_ratio_on_l1(&self, new_ratio: BaseTokenAPIRatio) -> anyhow::Result<()> {
        let Some(l1_params) = &self.l1_params else {
            return Ok(());
        };

        let max_attempts = self.config.l1_tx_sending_max_attempts;
        let sleep_duration = self.config.l1_tx_sending_sleep_duration();
        let mut prev_base_fee_per_gas: Option<u64> = None;
        let mut prev_priority_fee_per_gas: Option<u64> = None;
        let mut last_error = None;
        for attempt in 0..max_attempts {
            let (base_fee_per_gas, priority_fee_per_gas) =
                self.get_eth_fees(l1_params, prev_base_fee_per_gas, prev_priority_fee_per_gas);

            let start_time = Instant::now();
            let result = self
                .update_ratio_on_l1(l1_params, new_ratio, base_fee_per_gas, priority_fee_per_gas)
                .await;

            match result {
                Ok(x) => {
                    tracing::info!(
                        "Updated base token multiplier on L1: numerator {}, denominator {}, base_fee_per_gas {}, priority_fee_per_gas {}",
                        new_ratio.numerator.get(),
                        new_ratio.denominator.get(),
                        base_fee_per_gas,
                        priority_fee_per_gas
                    );
                    METRICS
                        .l1_gas_used
                        .set(x.unwrap_or(U256::zero()).low_u128() as u64);
                    METRICS.l1_update_latency[&OperationResultLabels {
                        result: OperationResult::Success,
                    }]
                        .observe(start_time.elapsed());

                    return Ok(());
                }
                Err(err) => {
                    tracing::info!(
                        "Failed to update base token multiplier on L1, attempt {}, base_fee_per_gas {}, priority_fee_per_gas {}: {}",
                        attempt,
                        base_fee_per_gas,
                        priority_fee_per_gas,
                        err
                    );
                    METRICS.l1_update_latency[&OperationResultLabels {
                        result: OperationResult::Failure,
                    }]
                        .observe(start_time.elapsed());

                    tokio::time::sleep(sleep_duration).await;
                    prev_base_fee_per_gas = Some(base_fee_per_gas);
                    prev_priority_fee_per_gas = Some(priority_fee_per_gas);
                    last_error = Some(err)
                }
            }
        }

        let error_message = "Failed to update base token multiplier on L1";
        Err(last_error
            .map(|x| x.context(error_message))
            .unwrap_or_else(|| anyhow::anyhow!(error_message)))
    }

    async fn retry_fetch_ratio(&self) -> anyhow::Result<BaseTokenAPIRatio> {
        let sleep_duration = self.config.price_fetching_sleep_duration();
        let max_retries = self.config.price_fetching_max_attempts;
        let mut last_error = None;

        for attempt in 0..max_retries {
            let start_time = Instant::now();
            match self
                .price_api_client
                .fetch_ratio(self.base_token_address)
                .await
            {
                Ok(ratio) => {
                    METRICS.external_price_api_latency[&OperationResultLabels {
                        result: OperationResult::Success,
                    }]
                        .observe(start_time.elapsed());
                    return Ok(ratio);
                }
                Err(err) => {
                    tracing::warn!(
                        "Attempt {}/{} to fetch ratio from external price api failed with err: {}. Retrying...",
                        attempt,
                        max_retries,
                        err
                    );
                    last_error = Some(err);
                    METRICS.external_price_api_latency[&OperationResultLabels {
                        result: OperationResult::Failure,
                    }]
                        .observe(start_time.elapsed());
                    sleep(sleep_duration).await;
                }
            }
        }
        let error_message = "Failed to fetch base token ratio after multiple attempts";
        Err(last_error
            .map(|x| x.context(error_message))
            .unwrap_or_else(|| anyhow::anyhow!(error_message)))
    }

    async fn persist_ratio(&self, api_ratio: BaseTokenAPIRatio) -> anyhow::Result<usize> {
        let mut conn = self
            .pool
            .connection_tagged("base_token_ratio_persister")
            .await
            .context("Failed to obtain connection to the database")?;

        let id = conn
            .base_token_dal()
            .insert_token_ratio(
                api_ratio.numerator,
                api_ratio.denominator,
                &api_ratio.ratio_timestamp.naive_utc(),
            )
            .await
            .context("Failed to insert base token ratio into the database")?;

        Ok(id)
    }

    async fn update_ratio_on_l1(
        &self,
        l1_params: &BaseTokenRatioPersisterL1Params,
        api_ratio: BaseTokenAPIRatio,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
    ) -> anyhow::Result<Option<U256>> {
        let fn_set_token_multiplier = l1_params
            .chain_admin_contract
            .function("setTokenMultiplier")
            .context("`setTokenMultiplier` function must be present in the ChainAdmin contract")?;

        let calldata = fn_set_token_multiplier
            .encode_input(
                &(
                    Token::Address(l1_params.diamond_proxy_contract_address),
                    Token::Uint(api_ratio.numerator.get().into()),
                    Token::Uint(api_ratio.denominator.get().into()),
                )
                    .into_tokens(),
            )
            .context("failed encoding `setTokenMultiplier` input")?;

        let nonce = (*l1_params.eth_client)
            .as_ref()
            .nonce_at_for_account(
                l1_params.token_multiplier_setter_account_address,
                BlockNumber::Pending,
            )
            .await
            .with_context(|| "failed getting transaction count")?
            .as_u64();

        let options = Options {
            gas: Some(U256::from(self.config.max_tx_gas)),
            nonce: Some(U256::from(nonce)),
            max_fee_per_gas: Some(U256::from(base_fee_per_gas + priority_fee_per_gas)),
            max_priority_fee_per_gas: Some(U256::from(priority_fee_per_gas)),
            ..Default::default()
        };

        let signed_tx = l1_params
            .eth_client
            .sign_prepared_tx_for_addr(
                calldata,
                l1_params.chain_admin_contract_address.unwrap(),
                options,
            )
            .await
            .context("cannot sign a `setTokenMultiplier` transaction")?;

        let hash = (*l1_params.eth_client)
            .as_ref()
            .send_raw_tx(signed_tx.raw_tx)
            .await
            .context("failed sending `setTokenMultiplier` transaction")?;

        let max_attempts = self.config.l1_receipt_checking_max_attempts;
        let sleep_duration = self.config.l1_receipt_checking_sleep_duration();
        for _i in 0..max_attempts {
            let maybe_receipt = (*l1_params.eth_client)
                .as_ref()
                .tx_receipt(hash)
                .await
                .context("failed getting receipt for `setTokenMultiplier` transaction")?;
            if let Some(receipt) = maybe_receipt {
                if receipt.status == Some(1.into()) {
                    return Ok(receipt.gas_used);
                }
                return Err(anyhow::Error::msg(format!(
                    "`setTokenMultiplier` transaction {:?} failed with status {:?}",
                    hex::encode(hash),
                    receipt.status
                )));
            } else {
                tokio::time::sleep(sleep_duration).await;
            }
        }

        Err(anyhow::Error::msg(format!(
            "Unable to retrieve `setTokenMultiplier` transaction status in {} attempts",
            max_attempts
        )))
    }
}
