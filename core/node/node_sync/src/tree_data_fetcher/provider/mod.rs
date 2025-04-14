use std::fmt;

use anyhow::Context;
use async_trait::async_trait;
use zksync_eth_client::EthInterface;
use zksync_types::{
    block::L2BlockHeader, web3, Address, L1BatchNumber, SLChainId, H256, U256, U64,
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    jsonrpsee::core::ClientError,
    namespaces::ZksNamespaceClient,
};

use super::{
    metrics::{ProcessingStage, TreeDataProviderSource, METRICS},
    TreeDataFetcherResult,
};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub(super) enum MissingData {
    /// The provider lacks a requested L1 batch.
    #[error("no requested L1 batch")]
    Batch,
    /// The provider lacks a root hash for a requested L1 batch; the batch itself is present on the provider.
    #[error("no root hash for L1 batch")]
    RootHash,
    #[error("possible chain reorg detected")]
    PossibleReorg,
}

pub(super) type TreeDataProviderResult = TreeDataFetcherResult<Result<H256, MissingData>>;

/// External provider of tree data, such as main node (via JSON-RPC).
#[async_trait]
pub(super) trait TreeDataProvider: fmt::Debug + Send + Sync + 'static {
    /// Fetches a state root hash for the L1 batch with the specified number.
    /// The method receives a header of the last L2 block in the batch, which can be used to check L1 batch consistency etc.
    ///
    /// It is guaranteed that this method will be called with monotonically increasing `number`s (although not necessarily sequential ones).
    async fn batch_details(
        &mut self,
        number: L1BatchNumber,
        last_l2_block: &L2BlockHeader,
    ) -> TreeDataProviderResult;
}

#[async_trait]
impl TreeDataProvider for Box<DynClient<L2>> {
    async fn batch_details(
        &mut self,
        number: L1BatchNumber,
        last_l2_block: &L2BlockHeader,
    ) -> TreeDataProviderResult {
        let Some(batch_details) = self
            .get_l1_batch_details(number)
            .rpc_context("get_l1_batch_details")
            .with_arg("number", &number)
            .await?
        else {
            return Ok(Err(MissingData::Batch));
        };

        // Check the local data correspondence.
        let remote_l2_block_hash = self
            .get_block_details(last_l2_block.number)
            .rpc_context("get_block_details")
            .with_arg("number", &last_l2_block.number)
            .await?
            .and_then(|block| block.base.root_hash);
        if remote_l2_block_hash != Some(last_l2_block.hash) {
            let last_l2_block_number = last_l2_block.number;
            let last_l2_block_hash = last_l2_block.hash;
            tracing::info!(
                "Fetched hash of the last L2 block #{last_l2_block_number} in L1 batch #{number} ({remote_l2_block_hash:?}) \
                 does not match the local one ({last_l2_block_hash:?}); this can be caused by a chain reorg"
            );
            return Ok(Err(MissingData::PossibleReorg));
        }

        Ok(batch_details.base.root_hash.ok_or(MissingData::RootHash))
    }
}

#[derive(Debug, Clone, Copy)]
struct PastL1BatchInfo {
    number: L1BatchNumber,
    l1_commit_block_number: U64,
    l1_commit_block_timestamp: U256,
    chain_id: SLChainId,
}

/// Provider of tree data loading it from L1 `BlockCommit` events emitted by the diamond proxy contract.
/// Should be used together with an L2 provider because L1 data can be missing for latest batches,
/// and the provider implementation uses assumptions that can break in some corner cases.
///
/// # Implementation details
///
/// To limit the range of L1 blocks for `eth_getLogs` calls, the provider assumes that an L1 block with a `BlockCommit` event
/// for a certain L1 batch is relatively close to L1 batch sealing. Thus, the provider finds an approximate L1 block number
/// for the event using binary search, or uses an L1 block number of the `BlockCommit` event for the previously queried L1 batch
/// (provided it's not too far behind the seal timestamp of the batch).
#[derive(Debug)]
pub(super) struct SLDataProvider {
    client: Box<dyn EthInterface>,
    chain_id: SLChainId,
    diamond_proxy_addr: Address,
    block_commit_signature: H256,
    past_l1_batch: Option<PastL1BatchInfo>,
}

impl SLDataProvider {
    /// Accuracy when guessing L1 block number by L1 batch timestamp.
    const L1_BLOCK_ACCURACY: U64 = U64([1_000]);
    /// Range of L1 blocks queried via `eth_getLogs`. Should be at least several times greater than
    /// `L1_BLOCK_ACCURACY`, but not large enough to trigger request limiting on the L1 RPC provider.
    const L1_BLOCK_RANGE: U64 = U64([20_000]);

    pub async fn new(
        l1_client: Box<dyn EthInterface>,
        l1_diamond_proxy_addr: Address,
    ) -> anyhow::Result<Self> {
        let l1_chain_id = l1_client.fetch_chain_id().await?;
        let block_commit_signature = zksync_contracts::hyperchain_contract()
            .event("BlockCommit")
            .context("missing `BlockCommit` event")?
            .signature();
        Ok(Self {
            client: l1_client,
            chain_id: l1_chain_id,
            diamond_proxy_addr: l1_diamond_proxy_addr,
            block_commit_signature,
            past_l1_batch: None,
        })
    }

    /// Guesses the number of an L1 block with a `BlockCommit` event for the specified L1 batch.
    /// The guess is based on the L1 batch seal timestamp.
    async fn guess_l1_commit_block_number(
        eth_client: &dyn EthInterface,
        l1_batch_seal_timestamp: u64,
    ) -> EnrichedClientResult<(U64, usize)> {
        let l1_batch_seal_timestamp = U256::from(l1_batch_seal_timestamp);
        let (latest_number, latest_timestamp) =
            Self::get_block(eth_client, web3::BlockNumber::Latest).await?;
        if latest_timestamp < l1_batch_seal_timestamp {
            return Ok((latest_number, 0)); // No better estimate at this point
        }
        let (earliest_number, earliest_timestamp) =
            Self::get_block(eth_client, web3::BlockNumber::Earliest).await?;
        if earliest_timestamp > l1_batch_seal_timestamp {
            return Ok((earliest_number, 0)); // No better estimate at this point
        }

        // At this point, we have `earliest_timestamp <= l1_batch_seal_timestamp <= latest_timestamp`.
        // Binary-search the range until we're sort of accurate.
        let mut steps = 0;
        let mut left = earliest_number;
        let mut right = latest_number;
        while left + Self::L1_BLOCK_ACCURACY < right {
            let middle = (left + right) / 2;
            let (_, middle_timestamp) =
                Self::get_block(eth_client, web3::BlockNumber::Number(middle)).await?;
            if middle_timestamp <= l1_batch_seal_timestamp {
                left = middle;
            } else {
                right = middle;
            }
            steps += 1;
        }
        Ok((left, steps))
    }

    /// Gets a block that should be present on L1.
    async fn get_block(
        eth_client: &dyn EthInterface,
        number: web3::BlockNumber,
    ) -> EnrichedClientResult<(U64, U256)> {
        let block = eth_client.block(number.into()).await?.ok_or_else(|| {
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
impl TreeDataProvider for SLDataProvider {
    async fn batch_details(
        &mut self,
        number: L1BatchNumber,
        last_l2_block: &L2BlockHeader,
    ) -> TreeDataProviderResult {
        let l1_batch_seal_timestamp = last_l2_block.timestamp;
        let from_block = self.past_l1_batch.and_then(|info| {
            assert!(
                info.number < number,
                "`batch_details()` must be called with monotonically increasing numbers"
            );
            if info.chain_id != self.chain_id {
                return None;
            }
            let threshold_timestamp = info.l1_commit_block_timestamp + Self::L1_BLOCK_RANGE.as_u64() / 2;
            if U256::from(l1_batch_seal_timestamp) > threshold_timestamp {
                tracing::debug!(
                    number = number.0,
                    "L1 batch #{number} seal timestamp ({l1_batch_seal_timestamp}) is too far ahead \
                     of the previous processed L1 batch ({info:?}); not using L1 batch info"
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
                let (approximate_block, steps) = Self::guess_l1_commit_block_number(
                    self.client.as_ref(),
                    l1_batch_seal_timestamp,
                )
                .await?;
                tracing::debug!(
                    number = number.0,
                    "Guessed L1 block number for L1 batch #{number} commit in {steps} binary search steps: {approximate_block}"
                );
                METRICS
                    .l1_commit_block_number_binary_search_steps
                    .observe(steps);
                // Subtract to account for imprecise L1 and L2 timestamps etc.
                approximate_block.saturating_sub(Self::L1_BLOCK_ACCURACY)
            }
        };

        let number_topic = H256::from_low_u64_be(number.0.into());
        let filter = web3::FilterBuilder::default()
            .address(vec![self.diamond_proxy_addr])
            .from_block(web3::BlockNumber::Number(from_block))
            .to_block(web3::BlockNumber::Number(from_block + Self::L1_BLOCK_RANGE))
            .topics(
                Some(vec![self.block_commit_signature]),
                Some(vec![number_topic]),
                None,
                None,
            )
            .build();
        let mut logs = self.client.logs(&filter).await?;
        logs.retain(|log| !log.is_removed() && log.block_number.is_some());

        match logs.as_slice() {
            [] => Ok(Err(MissingData::Batch)),
            [log] => {
                let root_hash = log.topics.get(2).copied().ok_or_else(|| {
                    let err = "Bogus `BlockCommit` event, does not have the root hash topic";
                    EnrichedClientError::new(ClientError::Custom(err.into()), "batch_details")
                        .with_arg("filter", &filter)
                        .with_arg("log", &log)
                })?;
                // `unwrap()` is safe due to the filtering above
                let l1_commit_block_number = log.block_number.unwrap();
                let diff = l1_commit_block_number.saturating_sub(from_block).as_u64();
                METRICS.l1_commit_block_number_from_diff.observe(diff);
                tracing::debug!(
                    "`BlockCommit` event for L1 batch #{number} is at block #{l1_commit_block_number}, \
                     {diff} block(s) after the `from` block from the filter"
                );

                let l1_commit_block = self.client.block(l1_commit_block_number.into()).await?;
                let l1_commit_block = l1_commit_block.ok_or_else(|| {
                    let err = "Block disappeared from L1 RPC provider";
                    EnrichedClientError::new(ClientError::Custom(err.into()), "batch_details")
                        .with_arg("number", &l1_commit_block_number)
                })?;
                self.past_l1_batch = Some(PastL1BatchInfo {
                    number,
                    l1_commit_block_number,
                    l1_commit_block_timestamp: l1_commit_block.timestamp,
                    chain_id: self.chain_id,
                });
                Ok(Ok(root_hash))
            }
            _ => {
                tracing::warn!(
                    "Non-unique `BlockCommit` event for L1 batch #{number} queried using {filter:?}, potentially as a result \
                     of a chain reorg: {logs:?}"
                );
                Ok(Err(MissingData::PossibleReorg))
            }
        }
    }
}

/// Data provider combining [`SLDataProvider`] with a fallback provider.
#[derive(Debug)]
pub(super) struct CombinedDataProvider {
    l1: Option<SLDataProvider>,
    // Generic to allow for tests.
    rpc: Box<dyn TreeDataProvider>,
}

impl CombinedDataProvider {
    pub fn new(fallback: impl TreeDataProvider) -> Self {
        Self {
            l1: None,
            rpc: Box::new(fallback),
        }
    }

    pub fn set_sl(&mut self, l1: SLDataProvider) {
        self.l1 = Some(l1);
    }
}

#[async_trait]
impl TreeDataProvider for CombinedDataProvider {
    #[tracing::instrument(skip(self, last_l2_block))]
    async fn batch_details(
        &mut self,
        number: L1BatchNumber,
        last_l2_block: &L2BlockHeader,
    ) -> TreeDataProviderResult {
        if let Some(l1) = &mut self.l1 {
            let stage_latency = METRICS.stage_latency[&ProcessingStage::FetchL1CommitEvent].start();
            let l1_result = l1.batch_details(number, last_l2_block).await;
            stage_latency.observe();

            match l1_result {
                Err(err) => {
                    if err.is_retriable() {
                        tracing::info!(
                            "Transient error calling L1 data provider: {:#}",
                            anyhow::Error::from(err)
                        );
                    } else {
                        tracing::warn!(
                            "Fatal error calling L1 data provider: {:#}",
                            anyhow::Error::from(err)
                        );
                        self.l1 = None;
                    }
                }
                Ok(Ok(root_hash)) => {
                    METRICS.root_hash_sources[&TreeDataProviderSource::L1CommitEvent].inc();
                    return Ok(Ok(root_hash));
                }
                Ok(Err(missing_data)) => {
                    tracing::info!("L1 data provider misses batch data: {missing_data}");
                    // No sense of calling the L1 provider in the future; the L2 provider will very likely get information
                    // about batches significantly faster.
                    self.l1 = None;
                }
            }
        }
        let stage_latency = METRICS.stage_latency[&ProcessingStage::FetchBatchDetailsRpc].start();
        let rpc_result = self.rpc.batch_details(number, last_l2_block).await;
        stage_latency.observe();
        if matches!(rpc_result, Ok(Ok(_))) {
            METRICS.root_hash_sources[&TreeDataProviderSource::BatchDetailsRpc].inc();
        }
        rpc_result
    }
}
