use std::{cmp::max, ops::Deref, sync::Arc};

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_eth_client::{BoundEthInterface, EnrichedClientError, Options};
use zksync_node_fee_model::l1_gas_price::TxParamsProvider;
use zksync_types::{
    api::Log,
    ethabi::{self},
    web3::{BlockId, BlockNumber, Filter, FilterBuilder},
    SLChainId, H256, U256,
};

use crate::types::{ClientError, ProofRequestIdentifier, ProofRequestParams};

#[derive(Clone, Debug)]
pub struct ProofManagerClient {
    pub(crate) client: Box<dyn BoundEthInterface>,
    pub(crate) gas_adjuster: Arc<dyn TxParamsProvider>,
    pub(crate) config: EthProofManagerConfig,
}

pub const RETRY_LIMIT: usize = 5;
const TOO_MANY_RESULTS_INFURA: &str = "query returned more than";
const TOO_MANY_RESULTS_ALCHEMY: &str = "response size exceeded";
const TOO_MANY_RESULTS_RETH: &str = "length limit exceeded";
const TOO_BIG_RANGE_RETH: &str = "query exceeds max block range";
const TOO_MANY_RESULTS_CHAINSTACK: &str = "range limit exceeded";
const REQUEST_REJECTED_503: &str = "Request rejected `503`";

#[async_trait]
pub trait EthProofManagerClient: 'static + std::fmt::Debug + Send + Sync {
    fn clone_boxed(&self) -> Box<dyn EthProofManagerClient>;

    async fn get_events_with_retry(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topics1: Option<Vec<H256>>,
        topics2: Option<Vec<H256>>,
        retries_left: usize,
    ) -> Result<Vec<Log>, EnrichedClientError>;

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>, EnrichedClientError>;

    async fn get_latest_block(&self) -> Result<u64, ClientError>;

    // function submitProofRequest(
    //     ProofRequestIdentifier calldata id,
    //     ProofRequestParams calldata params
    // )
    async fn submit_proof_request(
        &self,
        proof_request: ProofRequestIdentifier,
        proof_request_params: ProofRequestParams,
    ) -> Result<H256, ClientError>;

    // function submitProofValidationResult(ProofRequestIdentifier calldata id, bool isProofValid)
    async fn submit_proof_validation_result(
        &self,
        proof_request_identifier: ProofRequestIdentifier,
        is_proof_valid: bool,
    ) -> Result<H256, ClientError>;

    fn chain_id(&self) -> SLChainId;
}

impl ProofManagerClient {
    pub fn new(
        client: Box<dyn BoundEthInterface>,
        gas_adjuster: Arc<dyn TxParamsProvider>,
        config: EthProofManagerConfig,
    ) -> Self {
        Self {
            client,
            gas_adjuster,
            config,
        }
    }

    fn get_eth_fees(
        &self,
        prev_base_fee_per_gas: Option<u64>,
        prev_priority_fee_per_gas: Option<u64>,
    ) -> (u64, u64) {
        // Multiply by 2 to optimise for fast inclusion.
        let mut base_fee_per_gas = self.gas_adjuster.as_ref().get_base_fee(0) * 2;
        let mut priority_fee_per_gas = self.gas_adjuster.as_ref().get_priority_fee();
        if let Some(x) = prev_priority_fee_per_gas {
            // Increase `priority_fee_per_gas` by at least 20% to prevent "replacement transaction under-priced" error.
            priority_fee_per_gas = max(priority_fee_per_gas, (x * 6) / 5 + 1);
        }

        if let Some(x) = prev_base_fee_per_gas {
            // same for base_fee_per_gas but 10%
            base_fee_per_gas = max(base_fee_per_gas, x + (x / 10) + 1);
        }

        // Extra check to prevent sending transaction with extremely high priority fee.
        if priority_fee_per_gas > self.config.max_acceptable_priority_fee_in_gwei {
            panic!(
                "Extremely high value of priority_fee_per_gas is suggested: {}, while max acceptable is {}",
                priority_fee_per_gas,
                self.config.max_acceptable_priority_fee_in_gwei
            );
        }

        (base_fee_per_gas, priority_fee_per_gas)
    }

    async fn send_tx_with_retries(&self, calldata: Vec<u8>) -> Result<H256, ClientError> {
        let max_attempts = self.config.max_tx_sending_attempts;
        let sleep_duration = self.config.tx_sending_sleep;
        let mut prev_base_fee_per_gas: Option<u64> = None;
        let mut prev_priority_fee_per_gas: Option<u64> = None;
        let mut last_error = None;

        for attempt in 0..max_attempts {
            let (base_fee_per_gas, priority_fee_per_gas) =
                self.get_eth_fees(prev_base_fee_per_gas, prev_priority_fee_per_gas);

            let result = self
                .send_tx(calldata.clone(), base_fee_per_gas, priority_fee_per_gas)
                .await;

            if let Err(err) = result {
                tracing::info!(
                    "Failed to send transaction, attempt {}, base_fee_per_gas {}, priority_fee_per_gas {}: {}",
                    attempt,
                    base_fee_per_gas,
                    priority_fee_per_gas,
                    err);

                tokio::time::sleep(sleep_duration).await;
                prev_base_fee_per_gas = Some(base_fee_per_gas);
                prev_priority_fee_per_gas = Some(priority_fee_per_gas);
                last_error = Some(err)
            } else {
                return Ok(result.unwrap());
            }
        }

        let error_message = "Failed to send proof request";
        Err(last_error
            .map(|x| x.context(error_message))
            .unwrap_or_else(|| anyhow::anyhow!(error_message))
            .into())
    }

    async fn send_tx(
        &self,
        calldata: Vec<u8>,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
    ) -> anyhow::Result<H256> {
        let nonce = self
            .client
            .deref()
            .as_ref()
            .nonce_at_for_account(self.client.sender_account(), BlockNumber::Latest)
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

        let signed_tx = self
            .client
            .sign_prepared_tx_for_addr(calldata, self.client.contract_addr(), options)
            .await
            .context("cannot sign a transaction")?;

        let hash = self
            .client
            .deref()
            .as_ref()
            .send_raw_tx(signed_tx.raw_tx)
            .await
            .context("failed sending transaction")?;

        let max_attempts = self.config.tx_receipt_checking_max_attempts;
        let sleep_duration = self.config.tx_receipt_checking_sleep;
        for _i in 0..max_attempts {
            let maybe_receipt = self
                .client
                .deref()
                .as_ref()
                .tx_receipt(hash)
                .await
                .context(format!(
                    "failed getting receipt for transaction {}",
                    hex::encode(hash)
                ))?;
            if let Some(receipt) = maybe_receipt {
                tracing::info!("Transaction sent: {:?}", hex::encode(hash));

                if receipt.status == Some(1.into()) {
                    tracing::info!("Transaction confirmed: {:?}", hex::encode(hash));
                    return Ok(receipt.transaction_hash);
                }

                let reason = self
                    .client
                    .deref()
                    .as_ref()
                    .failure_reason(hash)
                    .await
                    .context(format!(
                        "failed getting failure reason of transaction {}",
                        hex::encode(hash)
                    ))?;

                return Err(anyhow::Error::msg(format!(
                    "Failed to send transaction {:?} failed with status {:?}, reason: {:?}",
                    hex::encode(hash),
                    receipt.status,
                    reason
                )));
            } else {
                tokio::time::sleep(sleep_duration).await;
            }
        }

        Err(anyhow::Error::msg(format!(
            "Unable to retrieve transaction status in {} attempts",
            max_attempts
        )))
    }
}

#[async_trait]
impl EthProofManagerClient for ProofManagerClient {
    fn clone_boxed(&self) -> Box<dyn EthProofManagerClient> {
        Box::new(self.clone())
    }

    /// Get the events with retries
    async fn get_events_with_retry(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topics1: Option<Vec<H256>>,
        topics2: Option<Vec<H256>>,
        retries_left: usize,
    ) -> Result<Vec<Log>, EnrichedClientError> {
        let filter = FilterBuilder::default()
            .from_block(from)
            .to_block(to)
            .topics(topics1.clone(), topics2.clone(), None, None)
            .address(vec![self.client.contract_addr()])
            .build();

        let mut result: Result<Vec<Log>, EnrichedClientError> = self.get_logs(filter).await;

        // This code is compatible with both Infura and Alchemy API providers.
        // Note: we don't handle rate-limits here - assumption is that we're never going to hit them.
        if let Err(err) = &result {
            tracing::warn!("Provider returned error message: {err}");
            let err_message = err.to_string();
            let err_code = if let jsonrpsee::core::ClientError::Call(err) = err.as_ref() {
                Some(err.code())
            } else {
                None
            };

            let should_retry = |err_code, err_message: String| {
                // All of these can be emitted by either API provider.
                err_code == Some(-32603)             // Internal error
                    || err_message.contains("failed")    // Server error
                    || err_message.contains("timed out") // Time-out error
            };

            // check whether the error is related to having too many results
            if err_message.contains(TOO_MANY_RESULTS_INFURA)
                || err_message.contains(TOO_MANY_RESULTS_ALCHEMY)
                || err_message.contains(TOO_MANY_RESULTS_RETH)
                || err_message.contains(TOO_BIG_RANGE_RETH)
                || err_message.contains(TOO_MANY_RESULTS_CHAINSTACK)
                || err_message.contains(REQUEST_REJECTED_503)
            {
                // get the numeric block ids
                let from_number = match from {
                    BlockNumber::Number(num) => num,
                    _ => {
                        // invalid variant
                        return result;
                    }
                };
                let to_number = match to {
                    BlockNumber::Number(num) => num,
                    BlockNumber::Latest => self.client.deref().as_ref().block_number().await?,
                    _ => {
                        // invalid variant
                        return result;
                    }
                };

                // divide range into two halves and recursively fetch them
                let mid = (from_number + to_number) / 2;

                // safety check to prevent infinite recursion (quite unlikely)
                if from_number >= mid {
                    tracing::warn!("Infinite recursion detected while getting events: from_number={from_number:?}, mid={mid:?}");
                    return result;
                }

                tracing::warn!("Splitting block range in half: {from:?} - {mid:?} - {to:?}");
                let mut first_half = self
                    .get_events_with_retry(
                        from,
                        BlockNumber::Number(mid),
                        topics1.clone(),
                        topics2.clone(),
                        RETRY_LIMIT,
                    )
                    .await?;
                let mut second_half = self
                    .get_events_with_retry(
                        BlockNumber::Number(mid + 1u64),
                        to,
                        topics1,
                        topics2,
                        RETRY_LIMIT,
                    )
                    .await?;

                first_half.append(&mut second_half);
                result = Ok(first_half);
            } else if should_retry(err_code, err_message) && retries_left > 0 {
                tracing::warn!("Retrying. Retries left: {retries_left}");
                result = self
                    .get_events_with_retry(from, to, topics1, topics2, retries_left - 1)
                    .await;
            }
        }

        result
    }

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>, EnrichedClientError> {
        Ok(self
            .client
            .deref()
            .as_ref()
            .logs(&filter)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Get the finalized block number from the L1 network.
    async fn get_latest_block(&self) -> Result<u64, ClientError> {
        let block = self
            .client
            .deref()
            .as_ref()
            .block(BlockId::Number(BlockNumber::Latest))
            .await?
            .ok_or_else(|| {
                let err = jsonrpsee::core::ClientError::Custom(
                    "Finalized block must be present on L1".into(),
                );
                let err = EnrichedClientError::new(err, "block");
                ClientError::Provider(err)
            })?;
        let block_number = block.number.ok_or_else(|| {
            let err =
                jsonrpsee::core::ClientError::Custom("Finalized block must contain number".into());
            let err = EnrichedClientError::new(err, "block").with_arg("block", &block);
            ClientError::Provider(err)
        })?;

        Ok(block_number.as_u64())
    }

    async fn submit_proof_request(
        &self,
        proof_request: ProofRequestIdentifier,
        proof_request_params: ProofRequestParams,
    ) -> Result<H256, ClientError> {
        let fn_submit_proof_request = self
            .client
            .contract()
            .function("submitProofRequest")
            .context(
                "`submitProofRequest` function must be present in the ProofManager contract",
            )?;

        let input = fn_submit_proof_request.encode_input(&[
            proof_request.into_tokens(),
            proof_request_params.into_tokens(),
        ])?;

        self.send_tx_with_retries(input).await
    }

    async fn submit_proof_validation_result(
        &self,
        proof_request_identifier: ProofRequestIdentifier,
        is_proof_valid: bool,
    ) -> Result<H256, ClientError> {
        let fn_submit_proof_validation_result = self.client.contract()
            .function("submitProofValidationResult")
            .context("`submitProofValidationResult` function must be present in the ProofManager contract")?;

        let input = fn_submit_proof_validation_result.encode_input(&[
            proof_request_identifier.into_tokens(),
            ethabi::Token::Bool(is_proof_valid),
        ])?;

        self.send_tx_with_retries(input).await
    }

    fn chain_id(&self) -> SLChainId {
        self.client.chain_id()
    }
}
