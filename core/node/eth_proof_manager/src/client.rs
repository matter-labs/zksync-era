use std::sync::Arc;

use anyhow::Context;
use zksync_eth_client::{
    clients::DynClient, web3_decl::client::Network, CallFunctionArgs, ContractCallError,
    EnrichedClientError, EthInterface,
};
use zksync_eth_watch::GetLogsClient;
use zksync_node_fee_model::l1_gas_price::TxParamsProvider;
use zksync_prover_interface::inputs::WitnessInputData;
use zksync_types::{
    api::Log,
    ethabi::{Address, Contract},
    protocol_version::ProtocolSemanticVersion,
    web3::{contract::Tokenize, BlockId, BlockNumber, FilterBuilder},
    L2ChainId, H256, U256,
};

use crate::types::{ClientError, ProofRequestIdentifier, ProofRequestParams};

pub struct EthProofManagerClient<Net: Network> {
    client: Box<DynClient<Net>>,
    gas_adjuster: Arc<dyn TxParamsProvider>,
    proof_manager_abi: Contract,
    proof_manager_address: Address,
    client_config: EthProofManagerConfig,
}

pub const RETRY_LIMIT: usize = 5;
const TOO_MANY_RESULTS_INFURA: &str = "query returned more than";
const TOO_MANY_RESULTS_ALCHEMY: &str = "response size exceeded";
const TOO_MANY_RESULTS_RETH: &str = "length limit exceeded";
const TOO_BIG_RANGE_RETH: &str = "query exceeds max block range";
const TOO_MANY_RESULTS_CHAINSTACK: &str = "range limit exceeded";
const REQUEST_REJECTED_503: &str = "Request rejected `503`";

impl<Net: Network> EthProofManagerClient<Net>
where
    Box<DynClient<Net>>: EthInterface + GetLogsClient,
{
    pub fn new(
        client: Box<DynClient<Net>>,
        gas_adjuster: Arc<dyn TxParamsProvider>,
        proof_manager_address: Address,
        proof_manager_abi: Contract,
    ) -> Self {
        Self {
            client,
            gas_adjuster,
            proof_manager_abi,
            proof_manager_address,
        }
    }

    pub async fn get_events_with_retry(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topics1: Option<Vec<H256>>,
        topics2: Option<Vec<H256>>,
        retries_left: usize,
    ) -> Result<Vec<Log>, ClientError> {
        let filter = FilterBuilder::default()
            .from_block(from)
            .to_block(to)
            .topics(topics1.clone(), topics2.clone(), None, None)
            .address(vec![self.proof_manager_address])
            .build();

        let mut result: Result<Vec<Log>, ClientError> =
            self.client.get_logs(filter).await.map_err(Into::into);

        // This code is compatible with both Infura and Alchemy API providers.
        // Note: we don't handle rate-limits here - assumption is that we're never going to hit them.
        if let Err(err) = &result {
            tracing::warn!("Provider returned error message: {err}");
            let err_message = err.as_ref().to_string();
            let err_code = if let ClientError::ContractCallError(ContractCallError::Call(err)) =
                err.as_ref()
            {
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
                    BlockNumber::Latest => self.client.block_number().await?,
                    _ => {
                        // invalid variant
                        return result.map_err(Into::into);
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

    pub async fn submit_proof_request(
        &self,
        chain_id: L2ChainId,
        batch_number: L1BatchNumber,
        protocol_version: ProtocolSemanticVersion,
        proof_inputs_url: String,
    ) -> Result<(), ClientError> {
        // params: id: ProofRequestIdentifier,
        // params: witness_input_data: ProofRequestParams,

        let fn_submit_proof_request = self
            .proof_manager_abi
            .function("submitProofRequest")
            .context(
                "`submitProofRequest` function must be present in the ProofManager contract",
            )?;

        let id = ProofRequestIdentifier {
            chain_id: chain_id.as_u64().into(),
            block_number: batch_number.into(),
        };

        let proof_request_params = ProofRequestParams {
            protocol_major: U256::zero(),
            protocol_minor: U256::from(protocol_version.minor() as u16),
            protocol_patch: U256::from(protocol_version.patch().0),
            proof_inputs_url,
            // todo: should be get from config
            timeout_after: U256(0),
            max_reward: U256(0),
        };

        let params = fn_submit_proof_request
            .encode_input(&(id.into_tokens(), proof_request_params.into_tokens()))?;

        CallFunctionArgs::new("submitProofRequest", params.into_tokens())
            .for_contract(self.proof_manager_address, &self.proof_manager_abi)
            .call(&self.client)
            .await
            .map_err(Into::into)
    }

    pub async fn submit_proof_validation_result(
        &self,
        chain_id: L2ChainId,
        batch_number: L1BatchNumber,
        is_proof_valid: bool,
    ) -> Result<(), ClientError> {
        let fn_submit_proof_validation_result = self.proof_manager_abi.function("submitProofValidationResult").context("`submitProofValidationResult` function must be present in the ProofManager contract")?;

        let id = ProofRequestIdentifier {
            chain_id: chain_id.as_u64().into(),
            block_number: batch_number.into(),
        };

        let params =
            fn_submit_proof_validation_result.encode_input(&(id.into_tokens(), is_proof_valid))?;

        CallFunctionArgs::new("submitProofValidationResult", params.into_tokens())
            .for_contract(self.proof_manager_address, &self.proof_manager_abi)
            .call(&self.client)
            .await
            .map_err(Into::into)
    }

    pub async fn get_finalized_block(&self) -> Result<u64, ClientError> {
        let block = self
            .client
            .block(BlockId::Number(BlockNumber::Finalized))
            .await?
            .ok_or_else(|| {
                let err = jsonrpsee::core::ClientError::Custom(
                    "Finalized block must be present on L1".into(),
                );
                let err = EnrichedClientError::new(err, "block");
                ClientError::ProviderError(err)
            })?;
        let block_number = block.number.ok_or_else(|| {
            let err =
                jsonrpsee::core::ClientError::Custom("Finalized block must contain number".into());
            let err = EnrichedClientError::new(err, "block").with_arg("block", &block);
            ClientError::ProviderError(err)
        })?;

        Ok(block_number.as_u64())
    }

    pub async fn send_transaction_retriable(
        &mut self,
        function_name: &str,
        params: Vec<Token>,
    ) -> anyhow::Result<()> {
        let max_attempts = l1_params.config.l1_tx_sending_max_attempts;
        let sleep_duration = l1_params.config.l1_tx_sending_sleep;
        let mut prev_base_fee_per_gas: Option<u64> = None;
        let mut prev_priority_fee_per_gas: Option<u64> = None;
        let mut last_error = None;
        for attempt in 0..max_attempts {
            let (base_fee_per_gas, priority_fee_per_gas) =
                self.get_eth_fees(l1_params, prev_base_fee_per_gas, prev_priority_fee_per_gas);

            let start_time = Instant::now();
            let result = self
                .do_update_l1(l1_params, new_ratio, base_fee_per_gas, priority_fee_per_gas)
                .await;

            match result {
                Ok(x) => {
                    tracing::info!(
                        "Updated base token multiplier on L1: numerator {}, denominator {}, base_fee_per_gas {}, priority_fee_per_gas {}, deviation {}",
                        new_ratio.numerator.get(),
                        new_ratio.denominator.get(),
                        base_fee_per_gas,
                        priority_fee_per_gas,
                        deviation
                    );
                    METRICS
                        .l1_gas_used
                        .set(x.unwrap_or(U256::zero()).low_u128() as u64);
                    METRICS.l1_update_latency[&OperationResultLabels {
                        result: OperationResult::Success,
                    }]
                        .observe(start_time.elapsed());
                    self.update_last_persisted_l1_ratio(
                        BigDecimal::from(new_ratio.numerator.get())
                            .div(BigDecimal::from(new_ratio.denominator.get())),
                    );

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

        let error_message = "Failed to send transaction on ProofManager contract";
        Err(last_error
            .map(|x| x.context(error_message))
            .unwrap_or_else(|| anyhow::anyhow!(error_message)))
    }

    async fn do_send_transaction(
        &self,
        function_name: &str,
        calldata: Vec<Token>,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
    ) -> anyhow::Result<Option<U256>> {
        let nonce = self
            .client
            .as_ref()
            .nonce_at_for_account(
                // todo: it should be owner here
                self.proof_manager_address,
                BlockNumber::Latest,
            )
            .await
            .with_context(|| "failed getting transaction count")?
            .as_u64();

        let options = Options {
            gas: Some(U256::from(self.client_config.max_tx_gas)),
            nonce: Some(U256::from(nonce)),
            max_fee_per_gas: Some(U256::from(base_fee_per_gas + priority_fee_per_gas)),
            max_priority_fee_per_gas: Some(U256::from(priority_fee_per_gas)),
            ..Default::default()
        };

        let signed_tx = self
            .client
            .sign_prepared_tx_for_addr(calldata, self.proof_manager_address, options)
            .await
            .context("cannot sign a `setTokenMultiplier` transaction")?;

        let hash = self
            .client
            .as_ref()
            .send_raw_tx(signed_tx.raw_tx)
            .await
            .context("failed sending `setTokenMultiplier` transaction")?;

        let max_attempts = self.client_config.l1_receipt_checking_max_attempts;
        let sleep_duration = self.client_config.l1_receipt_checking_sleep;
        for _i in 0..max_attempts {
            let maybe_receipt = self
                .client
                .as_ref()
                .tx_receipt(hash)
                .await
                .context("failed getting receipt for `setTokenMultiplier` transaction")?;
            if let Some(receipt) = maybe_receipt {
                if receipt.status == Some(1.into()) {
                    return Ok(receipt.gas_used);
                }
                let reason =
                    self.client.as_ref().failure_reason(hash).await.context(
                        "failed getting failure reason of `setTokenMultiplier` transaction",
                    )?;
                return Err(anyhow::Error::msg(format!(
                    "`setTokenMultiplier` transaction {:?} failed with status {:?}, reason: {:?}",
                    hex::encode(hash),
                    receipt.status,
                    reason
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
        if priority_fee_per_gas > l1_params.config.max_acceptable_priority_fee_in_gwei {
            panic!(
                "Extremely high value of priority_fee_per_gas is suggested: {}, while max acceptable is {}",
                priority_fee_per_gas,
                l1_params.config.max_acceptable_priority_fee_in_gwei
            );
        }

        (base_fee_per_gas, priority_fee_per_gas)
    }
}
