use std::{fmt, sync::Arc};

use zksync_contracts::verifier_contract;
use zksync_eth_client::{CallFunctionArgs, Error as EthClientError, EthInterface};
use zksync_l1_contract_interface::pre_boojum_verifier::old_l1_vk_commitment;
use zksync_types::{
    ethabi::{Contract, Token},
    web3::{
        self,
        contract::tokens::Detokenize,
        types::{BlockId, BlockNumber, FilterBuilder, Log},
    },
    Address, H256,
};

use super::metrics::METRICS;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Log parsing failed: {0}")]
    LogParse(String),
    #[error("Eth client error: {0}")]
    EthClient(#[from] EthClientError),
    #[error("Infinite recursion caused by too many responses")]
    InfiniteRecursion,
}

impl From<web3::contract::Error> for Error {
    fn from(err: web3::contract::Error) -> Self {
        Self::EthClient(err.into())
    }
}

#[async_trait::async_trait]
pub trait EthClient: 'static + fmt::Debug + Send + Sync {
    /// Returns events in a given block range.
    async fn get_events(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        retries_left: usize,
    ) -> Result<Vec<Log>, Error>;
    /// Returns finalized L1 block number.
    async fn finalized_block_number(&self) -> Result<u64, Error>;
    /// Returns scheduler verification key hash by verifier address.
    async fn scheduler_vk_hash(&self, verifier_address: Address) -> Result<H256, Error>;
    /// Sets list of topics to return events for.
    fn set_topics(&mut self, topics: Vec<H256>);
}

pub const RETRY_LIMIT: usize = 5;
const TOO_MANY_RESULTS_INFURA: &str = "query returned more than";
const TOO_MANY_RESULTS_ALCHEMY: &str = "response size exceeded";

#[derive(Debug)]
pub struct EthHttpQueryClient {
    client: Arc<dyn EthInterface>,
    topics: Vec<H256>,
    state_transition_chain_contract_addr: Address,
    /// Address of the `Governance` contract. It's optional because it is present only for post-boojum chains.
    /// If address is some then client will listen to events coming from it.
    governance_address: Option<Address>,
    verifier_contract_abi: Contract,
    confirmations_for_eth_event: Option<u64>,
}

impl EthHttpQueryClient {
    pub fn new(
        client: Arc<dyn EthInterface>,
        state_transition_chain_contract_addr: Address,
        governance_address: Option<Address>,
        confirmations_for_eth_event: Option<u64>,
    ) -> Self {
        tracing::debug!(
            "New eth client, zkSync addr: {:x}, governance addr: {:?}",
            state_transition_chain_contract_addr,
            governance_address
        );
        Self {
            client,
            topics: Vec::new(),
            state_transition_chain_contract_addr,
            governance_address,
            verifier_contract_abi: verifier_contract(),
            confirmations_for_eth_event,
        }
    }

    async fn get_filter_logs(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        topics: Vec<H256>,
    ) -> Result<Vec<Log>, Error> {
        let filter = FilterBuilder::default()
            .address(
                [
                    Some(self.state_transition_chain_contract_addr),
                    self.governance_address,
                ]
                .iter()
                .flatten()
                .copied()
                .collect(),
            )
            .from_block(from)
            .to_block(to)
            .topics(Some(topics), None, None, None)
            .build();

        self.client.logs(filter, "watch").await.map_err(Into::into)
    }
}

#[async_trait::async_trait]
impl EthClient for EthHttpQueryClient {
    async fn scheduler_vk_hash(&self, verifier_address: Address) -> Result<H256, Error> {
        // This is here for backward compatibility with the old verifier:
        // Legacy verifier returns the full verification key;
        // New verifier returns the hash of the verification key.

        let args = CallFunctionArgs::new("verificationKeyHash", ())
            .for_contract(verifier_address, self.verifier_contract_abi.clone());
        let vk_hash_tokens = self.client.call_contract_function(args).await;

        if let Ok(tokens) = vk_hash_tokens {
            Ok(H256::from_tokens(tokens)?)
        } else {
            let args = CallFunctionArgs::new("get_verification_key", ())
                .for_contract(verifier_address, self.verifier_contract_abi.clone());
            let vk = self.client.call_contract_function(args).await?;
            Ok(old_l1_vk_commitment(Token::from_tokens(vk)?))
        }
    }

    async fn get_events(
        &self,
        from: BlockNumber,
        to: BlockNumber,
        retries_left: usize,
    ) -> Result<Vec<Log>, Error> {
        let latency = METRICS.get_priority_op_events.start();
        let mut result = self.get_filter_logs(from, to, self.topics.clone()).await;

        // This code is compatible with both Infura and Alchemy API providers.
        // Note: we don't handle rate-limits here - assumption is that we're never going to hit them.
        if let Err(Error::EthClient(EthClientError::EthereumGateway(err))) = &result {
            tracing::warn!("Provider returned error message: {:?}", err);
            let err_message = err.to_string();
            let err_code = if let web3::Error::Rpc(err) = err {
                Some(err.code.code())
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
                    BlockNumber::Latest => self.client.block_number("watch").await?,
                    _ => {
                        // invalid variant
                        return result;
                    }
                };

                // divide range into two halves and recursively fetch them
                let mid = (from_number + to_number) / 2;

                // safety check to prevent infinite recursion (quite unlikely)
                if from_number >= mid {
                    return Err(Error::InfiniteRecursion);
                }
                tracing::warn!(
                    "Splitting block range in half: {:?} - {:?} - {:?}",
                    from,
                    mid,
                    to
                );
                let mut first_half = self
                    .get_events(from, BlockNumber::Number(mid), RETRY_LIMIT)
                    .await?;
                let mut second_half = self
                    .get_events(BlockNumber::Number(mid + 1u64), to, RETRY_LIMIT)
                    .await?;

                first_half.append(&mut second_half);
                result = Ok(first_half);
            } else if should_retry(err_code, err_message) && retries_left > 0 {
                tracing::warn!("Retrying. Retries left: {:?}", retries_left);
                result = self.get_events(from, to, retries_left - 1).await;
            }
        }

        latency.observe();
        result
    }

    async fn finalized_block_number(&self) -> Result<u64, Error> {
        if let Some(confirmations) = self.confirmations_for_eth_event {
            let latest_block_number = self.client.block_number("watch").await?.as_u64();
            Ok(latest_block_number.saturating_sub(confirmations))
        } else {
            self.client
                .block(BlockId::Number(BlockNumber::Finalized), "watch")
                .await
                .map_err(Into::into)
                .map(|res| {
                    res.expect("Finalized block must be present on L1")
                        .number
                        .expect("Finalized block must contain number")
                        .as_u64()
                })
        }
    }

    fn set_topics(&mut self, topics: Vec<H256>) {
        self.topics = topics;
    }
}
