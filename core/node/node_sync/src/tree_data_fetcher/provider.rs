use std::fmt;

use anyhow::Context;
use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_eth_client::EthInterface;
use zksync_types::{web3, Address, L1BatchNumber, H256, U256, U64};
use zksync_web3_decl::{
    client::{DynClient, L1, L2},
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    jsonrpsee::core::ClientError,
    namespaces::ZksNamespaceClient,
};

use super::TreeDataFetcherResult;

#[derive(Debug, thiserror::Error)]
pub(crate) enum MissingData {
    /// The provider lacks a requested L1 batch.
    #[error("no requested L1 batch")]
    Batch,
    /// The provider lacks a root hash for a requested L1 batch; the batch itself is present on the provider.
    #[error("no root hash for L1 batch")]
    RootHash,
}

/// External provider of tree data, such as main node (via JSON-RPC).
#[async_trait]
pub(crate) trait TreeDataProvider: fmt::Debug + Send + Sync + 'static {
    /// Fetches a state root hash for the L1 batch with the specified number.
    async fn batch_details(
        &mut self,
        number: L1BatchNumber,
    ) -> TreeDataFetcherResult<Result<H256, MissingData>>;
}

#[async_trait]
impl TreeDataProvider for Box<DynClient<L2>> {
    async fn batch_details(
        &mut self,
        number: L1BatchNumber,
    ) -> TreeDataFetcherResult<Result<H256, MissingData>> {
        let Some(batch_details) = self
            .get_l1_batch_details(number)
            .rpc_context("get_l1_batch_details")
            .with_arg("number", &number)
            .await?
        else {
            return Ok(Err(MissingData::Batch));
        };
        Ok(batch_details.base.root_hash.ok_or(MissingData::RootHash))
    }
}

#[derive(Debug, Clone, Copy)]
struct PastL1BatchInfo {
    number: L1BatchNumber,
    l1_commit_block_number: U64,
    l1_commit_block_timestamp: U256,
}

/// Provider of tree data loading it from L1 `BlockCommit` events emitted by the diamond proxy contract.
/// Should be used together with an L2 provider because L1 data can be missing for latest batches,
/// and the provider implementation uses assumptions that can break in some corner cases.
#[derive(Debug)]
pub(crate) struct L1DataProvider {
    pool: ConnectionPool<Core>,
    eth_client: Box<DynClient<L1>>,
    diamond_proxy_address: Address,
    block_commit_signature: H256,
    past_l1_batch: Option<PastL1BatchInfo>,
}

impl L1DataProvider {
    /// Accuracy when guessing L1 block number by L1 batch timestamp.
    const L1_BLOCK_ACCURACY: u64 = 1_000;
    /// Range of L1 blocks queried via `eth_getLogs`. Should be at least several times greater than
    /// `L1_BLOCK_ACCURACY`.
    const L1_BLOCK_RANGE: U64 = U64([20_000]);

    pub fn new(
        pool: ConnectionPool<Core>,
        eth_client: Box<DynClient<L1>>,
        diamond_proxy_address: Address,
    ) -> anyhow::Result<Self> {
        let block_commit_signature = zksync_contracts::hyperchain_contract()
            .event("BlockCommit")
            .context("missing `BlockCommit` event")?
            .signature();
        Ok(Self {
            pool,
            eth_client,
            diamond_proxy_address,
            block_commit_signature,
            past_l1_batch: None,
        })
    }

    async fn l1_batch_timestamp(&self, number: L1BatchNumber) -> anyhow::Result<u64> {
        let mut storage = self.pool.connection_tagged("tree_data_fetcher").await?;
        let header = storage.blocks_dal().get_l1_batch_header(number).await?;
        let header =
            header.with_context(|| format!("L1 batch #{number} disappeared from Postgres"))?;
        Ok(header.timestamp)
    }

    /// Guesses the number of an L1 block with a `BlockCommit` event for the specified L1 batch.
    /// The guess is based on the L1 batch timestamp.
    async fn guess_l1_commit_block_number(
        &self,
        l1_batch_timestamp: u64,
    ) -> EnrichedClientResult<U64> {
        let l1_batch_timestamp = U256::from(l1_batch_timestamp);
        let (latest_number, latest_timestamp) = self.get_block(web3::BlockNumber::Latest).await?;
        if latest_timestamp < l1_batch_timestamp {
            return Ok(latest_number); // No better estimate at this point
        }
        let (earliest_number, earliest_timestamp) =
            self.get_block(web3::BlockNumber::Earliest).await?;
        if earliest_timestamp > l1_batch_timestamp {
            return Ok(earliest_number); // No better estimate at this point
        }

        // At this point, we have earliest_timestamp <= l1_batch_timestamp <= latest_timestamp.
        // Binary-search the range until we're sort of accurate.
        let mut left = earliest_number;
        let mut right = latest_number;
        while left + U64::from(Self::L1_BLOCK_ACCURACY) < right {
            let middle = (left + right) / 2;
            let (_, middle_timestamp) = self.get_block(web3::BlockNumber::Number(middle)).await?;
            if middle_timestamp <= l1_batch_timestamp {
                left = middle;
            } else {
                right = middle;
            }
        }
        Ok(left)
    }

    async fn get_block(&self, number: web3::BlockNumber) -> EnrichedClientResult<(U64, U256)> {
        let block = self.eth_client.block(number.into()).await?.ok_or_else(|| {
            let err = "block is missing on L1 RPC provider";
            EnrichedClientError::new(ClientError::Custom(err.into()), "get_block")
                .with_arg("number", &number)
        })?;
        let number = block.number.ok_or_else(|| {
            let err = "block is missing a number";
            EnrichedClientError::new(ClientError::Custom(err.into()), "get_block")
                .with_arg("number", &number)
        })?;
        Ok((number, block.timestamp))
    }
}

#[async_trait]
impl TreeDataProvider for L1DataProvider {
    async fn batch_details(
        &mut self,
        number: L1BatchNumber,
    ) -> TreeDataFetcherResult<Result<H256, MissingData>> {
        let l1_batch_timestamp = self.l1_batch_timestamp(number).await?;
        let from_block = self.past_l1_batch.and_then(|info| {
            assert!(
                info.number < number,
                "`batch_details()` must be called with monotonically increasing numbers"
            );
            let threshold_timestamp = info.l1_commit_block_timestamp + Self::L1_BLOCK_RANGE.as_u64() / 2;
            if U256::from(l1_batch_timestamp) > threshold_timestamp {
                tracing::debug!(
                    number = number.0,
                    "L1 batch #{number} timestamp ({l1_batch_timestamp}) is too far ahead of the previous processed L1 batch ({info:?}); \
                     dropping L1 batch info"
                );
                None
            } else {
                // This is an exact lower boundary: L1 batches are committed in order
                Some(info.l1_commit_block_number)
            }
        });

        let from_block = match from_block {
            Some(number) => number,
            None => {
                let approximate_block = self
                    .guess_l1_commit_block_number(l1_batch_timestamp)
                    .await?;
                tracing::debug!(
                    number = number.0,
                    "Guessed L1 block number for L1 batch #{number} commit: {approximate_block:?}"
                );
                approximate_block.saturating_sub(5_000.into()) // FIXME: extract to a constant?
            }
        };

        let number_topic = H256::from_low_u64_be(number.0.into());
        let filter = web3::FilterBuilder::default()
            .address(vec![self.diamond_proxy_address])
            .from_block(web3::BlockNumber::Number(from_block))
            .to_block(web3::BlockNumber::Number(from_block + Self::L1_BLOCK_RANGE))
            .topics(
                Some(vec![self.block_commit_signature]),
                Some(vec![number_topic]),
                None,
                None,
            )
            .build();
        let mut logs = self.eth_client.logs(&filter).await?;
        logs.retain(|log| !log.is_removed() && log.block_number.is_some());

        match logs.as_slice() {
            [] => Ok(Err(MissingData::Batch)),
            [log] => {
                let root_hash_topic = log.topics.get(2).copied().ok_or_else(|| {
                    let err = "Bogus `BlockCommit` event, does not have the root hash topic";
                    EnrichedClientError::new(ClientError::Custom(err.into()), "batch_details")
                        .with_arg("filter", &filter)
                })?;
                // `unwrap()` is safe due to the filtering above
                let l1_commit_block_number = log.block_number.unwrap();

                let l1_commit_block = self.eth_client.block(l1_commit_block_number.into()).await?;
                let l1_commit_block = l1_commit_block.ok_or_else(|| {
                    let err = "Block disappeared from L1 RPC provider";
                    EnrichedClientError::new(ClientError::Custom(err.into()), "batch_details")
                        .with_arg("number", &l1_commit_block_number)
                })?;
                self.past_l1_batch = Some(PastL1BatchInfo {
                    number,
                    l1_commit_block_number,
                    l1_commit_block_timestamp: l1_commit_block.timestamp,
                });
                Ok(Ok(root_hash_topic))
            }
            _ => {
                tracing::warn!("Non-unique `BlockCommit` event for L1 batch #{number} queried using {filter:?}: {logs:?}");
                Ok(Err(MissingData::RootHash))
            }
        }
    }
}
