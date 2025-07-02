use std::ops::Deref;

use anyhow::Context as _;
use async_trait::async_trait;
use zksync_contracts::proof_manager_contract;
use zksync_eth_client::{BoundEthInterface, EnrichedClientError, Options};
use zksync_types::{
    api::Log,
    ethabi::{self, Address, Contract},
    web3::{BlockId, BlockNumber, Filter, FilterBuilder},
    H256,
};

use crate::types::{ClientError, ProofRequestIdentifier, ProofRequestParams};

#[derive(Clone, Debug)]
pub struct ProofManagerClient {
    client: Box<dyn BoundEthInterface>,
    proof_manager_abi: Contract,
    proof_manager_address: Address,
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
    async fn get_events_with_retry(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topics1: Option<Vec<H256>>,
        topics2: Option<Vec<H256>>,
        retries_left: usize,
    ) -> Result<Vec<Log>, EnrichedClientError>;

    async fn get_logs(&self, filter: Filter) -> Result<Vec<Log>, EnrichedClientError>;

    async fn get_finalized_block(&self) -> Result<u64, ClientError>;

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
}

impl ProofManagerClient {
    pub fn new(client: Box<dyn BoundEthInterface>, proof_manager_address: Address) -> Self {
        Self {
            client,
            proof_manager_abi: proof_manager_contract(),
            proof_manager_address,
        }
    }
}

#[async_trait]
impl EthProofManagerClient for ProofManagerClient {
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
            .address(vec![self.proof_manager_address])
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
    async fn get_finalized_block(&self) -> Result<u64, ClientError> {
        let block = self
            .client
            .deref()
            .as_ref()
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

    async fn submit_proof_request(
        &self,
        proof_request: ProofRequestIdentifier,
        proof_request_params: ProofRequestParams,
    ) -> Result<H256, ClientError> {
        let fn_submit_proof_request = self
            .proof_manager_abi
            .function("submitProofRequest")
            .context(
                "`submitProofRequest` function must be present in the ProofManager contract",
            )?;

        let input = fn_submit_proof_request.encode_input(&[
            proof_request.into_tokens(),
            proof_request_params.into_tokens(),
        ])?;

        let tx = self
            .client
            .sign_prepared_tx_for_addr(input, self.proof_manager_address, Options::default())
            .await?;

        Ok(tx.hash)
    }

    async fn submit_proof_validation_result(
        &self,
        proof_request_identifier: ProofRequestIdentifier,
        is_proof_valid: bool,
    ) -> Result<H256, ClientError> {
        let fn_submit_proof_validation_result = self
            .proof_manager_abi
            .function("submitProofValidationResult")
            .context("`submitProofValidationResult` function must be present in the ProofManager contract")?;

        let input = fn_submit_proof_validation_result.encode_input(&[
            proof_request_identifier.into_tokens(),
            ethabi::Token::Bool(is_proof_valid),
        ])?;

        let tx = self
            .client
            .sign_prepared_tx_for_addr(input, self.proof_manager_address, Options::default())
            .await?;

        Ok(tx.hash)
    }
}
