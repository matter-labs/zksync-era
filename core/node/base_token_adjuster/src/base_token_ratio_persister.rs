use std::{fmt::Debug, sync::Arc, time::Duration};

use anyhow::Context as _;
use tokio::{sync::watch, time::sleep};
use zksync_config::configs::base_token_adjuster::BaseTokenAdjusterConfig;
use zksync_contracts::chain_admin_contract;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::{BoundEthInterface, EthInterface, Options};
use zksync_external_price_api::PriceAPIClient;
use zksync_node_fee_model::l1_gas_price::L1TxParamsProvider;
use zksync_types::{
    base_token_ratio::BaseTokenAPIRatio,
    ethabi::{Contract, Token},
    web3::{contract::Tokenize, BlockNumber},
    Address, U256,
};

#[derive(Debug, Clone)]
pub struct BaseTokenRatioPersister {
    pool: ConnectionPool<Core>,
    config: BaseTokenAdjusterConfig,
    base_token_address: Address,
    price_api_client: Arc<dyn PriceAPIClient>,
    eth_client: Box<dyn BoundEthInterface>,
    gas_adjuster: Arc<dyn L1TxParamsProvider>,
    //TODO: use multiplier setter account
    base_token_adjuster_account_address: Address,
    chain_admin_contract: Contract,
    diamond_proxy_contract_address: Address,
    chain_admin_contract_address: Option<Address>,
}

impl BaseTokenRatioPersister {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pool: ConnectionPool<Core>,
        config: BaseTokenAdjusterConfig,
        base_token_address: Address,
        price_api_client: Arc<dyn PriceAPIClient>,
        eth_client: Box<dyn BoundEthInterface>,
        gas_adjuster: Arc<dyn L1TxParamsProvider>,
        base_token_adjuster_account_address: Address,
        diamond_proxy_contract_address: Address,
        chain_admin_contract_address: Option<Address>,
    ) -> Self {
        let chain_admin_contract = chain_admin_contract();

        Self {
            pool,
            config,
            base_token_address,
            price_api_client,
            eth_client,
            gas_adjuster,
            base_token_adjuster_account_address,
            chain_admin_contract,
            diamond_proxy_contract_address,
            chain_admin_contract_address,
        }
    }

    /// Main loop for the base token ratio persister.
    /// Orchestrates fetching a new ratio, persisting it, and conditionally updating the L1 with it.
    pub async fn run(&mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(self.config.price_polling_interval());

        while !*stop_receiver.borrow_and_update() {
            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }

            if let Err(err) = self.loop_iteration().await {
                return Err(err)
                    .context("Failed to execute a base_token_ratio_persister loop iteration");
            }
        }

        tracing::info!("Stop signal received, base_token_ratio_persister is shutting down");
        Ok(())
    }

    async fn loop_iteration(&self) -> anyhow::Result<()> {
        // TODO(PE-148): Consider shifting retry upon adding external API redundancy.
        let new_ratio = self.retry_fetch_ratio().await?;
        self.persist_ratio(new_ratio).await?;
        self.send_ratio_to_l1(new_ratio).await
    }

    async fn retry_fetch_ratio(&self) -> anyhow::Result<BaseTokenAPIRatio> {
        let sleep_duration = Duration::from_secs(1);
        let max_retries = 5;
        let mut attempts = 0;

        loop {
            match self
                .price_api_client
                .fetch_ratio(self.base_token_address)
                .await
            {
                Ok(ratio) => {
                    return Ok(ratio);
                }
                Err(err) if attempts < max_retries => {
                    attempts += 1;
                    tracing::warn!(
                        "Attempt {}/{} to fetch ratio from coingecko failed with err: {}. Retrying...",
                        attempts,
                        max_retries,
                        err
                    );
                    sleep(sleep_duration).await;
                }
                Err(err) => {
                    return Err(err)
                        .context("Failed to fetch base token ratio after multiple attempts");
                }
            }
        }
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

    async fn send_ratio_to_l1(&self, api_ratio: BaseTokenAPIRatio) -> anyhow::Result<()> {
        let fn_set_token_multiplier = self
            .chain_admin_contract
            .function("setTokenMultiplier")
            .context("`setTokenMultiplier` function must be present in the ChainAdmin contract")?;

        let calldata = fn_set_token_multiplier
            .encode_input(
                &(
                    Token::Address(self.diamond_proxy_contract_address),
                    Token::Uint(api_ratio.numerator.get().into()),
                    Token::Uint(api_ratio.denominator.get().into()),
                )
                    .into_tokens(),
            )
            .context("failed encoding `setTokenMultiplier` input")?;

        let nonce = (*self.eth_client)
            .as_ref()
            .nonce_at_for_account(
                self.base_token_adjuster_account_address,
                BlockNumber::Pending,
            )
            .await
            .with_context(|| "failed getting transaction count")?
            .as_u64();

        let gas_price = (*self.eth_client)
            .as_ref()
            .get_gas_price()
            .await
            .with_context(|| "failed getting gas price")?
            .as_u64()
            * 2;

        let base_fee_per_gas = self.gas_adjuster.as_ref().get_base_fee(0);
        let priority_fee_per_gas = self.gas_adjuster.as_ref().get_priority_fee();

        let options = Options {
            gas: Some(U256::from(self.config.max_tx_gas)),
            nonce: Some(U256::from(nonce)),
            gas_price: Some(gas_price.into()),
            max_fee_per_gas: Some(U256::from(base_fee_per_gas + priority_fee_per_gas)),
            max_priority_fee_per_gas: Some(U256::from(priority_fee_per_gas)),
            ..Default::default()
        };

        let signed_tx = self
            .eth_client
            .sign_prepared_tx_for_addr(
                calldata,
                self.chain_admin_contract_address.unwrap(),
                options,
            )
            .await
            .context("cannot sign a `setTokenMultiplier` transaction")?;

        let hash = (*self.eth_client)
            .as_ref()
            .send_raw_tx(signed_tx.raw_tx)
            .await
            .context("failed sending `setTokenMultiplier` transaction")?;

        let max_attempts = self.config.persister_l1_receipt_checking_max_attempts();
        let sleep_duration = self.config.persister_l1_receipt_checking_sleep_duration();
        for _i in 0..max_attempts {
            let maybe_receipt = (*self.eth_client)
                .as_ref()
                .tx_receipt(hash)
                .await
                .context("failed getting receipt for `setTokenMultiplier` transaction")?;
            if let Some(receipt) = maybe_receipt {
                if receipt.status == Some(1.into()) {
                    return Ok(());
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
