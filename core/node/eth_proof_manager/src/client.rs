use zksync_eth_client::{
    clients::DynClient, web3_decl::client::Network, CallFunctionArgs, ContractCallError,
    EnrichedClientError, EthInterface,
};
use zksync_eth_watch::GetLogsClient;
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

impl<Net: Network> EthProofManagerClient<Net>
where
    Box<DynClient<Net>>: EthInterface + GetLogsClient,
{
    pub fn new(
        client: Box<DynClient<Net>>,
        proof_manager_address: Address,
        proof_manager_abi: Contract,
    ) -> Self {
        Self {
            client,
            proof_manager_abi,
            proof_manager_address,
        }
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

        let params = (id, proof_request_params);

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
        // id: ProofRequestIdentifier,
        // is_proof_valid: bool,

        let id = ProofRequestIdentifier {
            chain_id: chain_id.as_u64().into(),
            block_number: batch_number.into(),
        };

        let params = (id, is_proof_valid);

        CallFunctionArgs::new("submitProofValidationResult", params)
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
            .address(vec![self.proof_manager_address]).build();

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
}
