//! Component responsible for fetching miniblock precommits.

use std::{fmt, time::Duration};

use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::EthInterface;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{
    web3::{BlockId, BlockNumber},
    L2BlockNumber, H256,
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    namespaces::ZksNamespaceClient,
};

use super::metrics::{FetchStage, FETCHER_METRICS};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
enum FetcherError {
    #[error("JSON-RPC error communicating with main node")]
    Web3(#[from] EnrichedClientError),
    #[error("Internal error")]
    Internal(#[from] anyhow::Error),
}

impl From<zksync_dal::DalError> for FetcherError {
    fn from(err: zksync_dal::DalError) -> Self {
        Self::Internal(err.into())
    }
}

#[async_trait]
trait MainNodeClient: fmt::Debug + Send + Sync {
    async fn get_miniblock_precommit_hash(
        &self,
        number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<H256>>;

    async fn get_safe_block(&self) -> EnrichedClientResult<L2BlockNumber>;
}

#[async_trait]
impl MainNodeClient for Box<DynClient<L2>> {
    async fn get_miniblock_precommit_hash(
        &self,
        number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<H256>> {
        let request_latency = FETCHER_METRICS.requests[&FetchStage::GetMiniblockDetails].start();
        let details = self
            .get_block_details(number)
            .rpc_context("get_block_details")
            .with_arg("number", &number)
            .await?;
        request_latency.observe();
        Ok(details.and_then(|details| details.base.precommit_tx_hash))
    }

    async fn get_safe_block(&self) -> EnrichedClientResult<L2BlockNumber> {
        let request_latency = FETCHER_METRICS.requests[&FetchStage::GetSafeBlock].start();
        let safe_block = self.block(BlockId::Number(BlockNumber::Safe)).await?;
        request_latency.observe();
        Ok(L2BlockNumber(
            safe_block
                .expect("Safe block is always returned by zkstack server")
                .number
                .expect("Safe block must contain number")
                .as_u32(),
        ))
    }
}

/// Cursor for the last processed miniblock with precommits
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
struct FetcherCursor {
    last_processed_miniblock: L2BlockNumber,
}

impl FetcherCursor {
    async fn new(storage: &mut Connection<'_, Core>) -> anyhow::Result<Self> {
        // Check if we have any precommits already
        let last_block_with_precommit = storage
            .blocks_dal()
            .get_last_miniblock_with_precommit()
            .await?;

        // If we have precommits, start from the last one, otherwise start from the beginning
        let last_processed_miniblock = last_block_with_precommit.unwrap_or(L2BlockNumber(0));

        Ok(Self {
            last_processed_miniblock,
        })
    }
}

/// Component responsible for fetching miniblock precommits from the main node.
/// It periodically checks the "safe" block reported by the main node and fetches precommits
/// for miniblocks up to that safe block.
#[derive(Debug)]
pub struct MiniblockPrecommitFetcher {
    client: Box<dyn MainNodeClient>,
    pool: ConnectionPool<Core>,
    health_updater: HealthUpdater,
    sleep_interval: Duration,
}

impl MiniblockPrecommitFetcher {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(client: Box<DynClient<L2>>, pool: ConnectionPool<Core>) -> Self {
        Self::from_parts(
            Box::new(client.for_component("miniblock_precommit_fetcher")),
            pool,
            Self::DEFAULT_SLEEP_INTERVAL,
        )
    }

    fn from_parts(
        client: Box<dyn MainNodeClient>,
        pool: ConnectionPool<Core>,
        sleep_interval: Duration,
    ) -> Self {
        Self {
            client,
            pool,
            health_updater: ReactiveHealthCheck::new("miniblock_precommit_fetcher").1,
            sleep_interval,
        }
    }

    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut storage = self.pool.connection_tagged("sync_layer").await?;
        let mut cursor = FetcherCursor::new(&mut storage).await?;
        drop(storage);
        tracing::info!(
            "Initialized miniblock precommit fetcher cursor: {:?}",
            cursor
        );
        self.health_updater
            .update(Health::from(HealthStatus::Ready).with_details(cursor));

        while !*stop_receiver.borrow_and_update() {
            match self.fetch_precommits(&mut cursor).await {
                Ok(fetched_any) => {
                    if !fetched_any {
                        // If we didn't fetch any precommits, wait a bit before trying again
                        if tokio::time::timeout(self.sleep_interval, stop_receiver.changed())
                            .await
                            .is_ok()
                        {
                            break;
                        }
                    }
                }
                Err(FetcherError::Web3(err)) => {
                    tracing::warn!("Failed to fetch precommits from the main node: {}", err);
                    // Wait before retrying after an error
                    if tokio::time::timeout(self.sleep_interval, stop_receiver.changed())
                        .await
                        .is_ok()
                    {
                        break;
                    }
                }
                Err(FetcherError::Internal(err)) => return Err(err),
            }
        }

        tracing::info!("Stop request received, exiting the miniblock precommit fetcher routine");
        Ok(())
    }

    const MAX_BLOCKS_PER_ITERATION: u32 = 1000;

    /// Fetches precommits for blocks from the cursor up to the safe block
    /// Returns true if any precommits were fetched
    async fn fetch_precommits(&self, cursor: &mut FetcherCursor) -> Result<bool, FetcherError> {
        // Get the current safe block from the main node
        let safe_block = self.client.get_safe_block().await?;

        tracing::debug!(
            "Safe block is {}, last processed block is {}",
            safe_block.0,
            cursor.last_processed_miniblock.0
        );

        // Check if there are new blocks to process
        if safe_block <= cursor.last_processed_miniblock {
            return Ok(false);
        }

        // Calculate end block with limit to avoid processing too many blocks at once
        let end_block = std::cmp::min(
            safe_block,
            L2BlockNumber(cursor.last_processed_miniblock.0 + Self::MAX_BLOCKS_PER_ITERATION),
        );

        if end_block > safe_block {
            tracing::info!(
                "Processing blocks from {} to {} (limited from {})",
                cursor.last_processed_miniblock.0 + 1,
                end_block.0,
                safe_block.0
            );
        }

        let mut blocks_processed = 0;
        let mut precommits_found = 0;
        let mut fetched_any = false;

        // Process blocks from the next one after the last processed up to the calculated end block
        let mut current_block = cursor.last_processed_miniblock + 1;
        while current_block <= end_block {
            let precommit_hash = match self
                .client
                .get_miniblock_precommit_hash(current_block)
                .await
            {
                Ok(precommit_hash) => precommit_hash,
                Err(err) => {
                    tracing::warn!("Failed to get precommit hash {}", err);
                    // FIXME soft fail on error?
                    self.health_updater
                        .update(Health::from(HealthStatus::Ready).with_details(*cursor));
                    return Err(err.into());
                }
            };

            blocks_processed += 1;

            match precommit_hash {
                Some(precommit_hash) => {
                    let mut connection = self.pool.connection_tagged("sync_layer").await?;

                    tracing::info!("Found precommitment for block {}", current_block.0);

                    // Store the precommitment in the database
                    connection
                        .eth_sender_dal()
                        .insert_pending_received_precommit_eth_tx(
                            current_block,
                            precommit_hash,
                            None,
                        )
                        .await?;

                    precommits_found += 1;
                    fetched_any = true;

                    // Update cursor regardless of whether we found a precommitment
                    cursor.last_processed_miniblock = current_block;
                    current_block += 1;
                }
                None => {
                    tracing::debug!("No precommitment for block {}", current_block.0);
                    break;
                }
            }
        }

        if blocks_processed > 0 {
            tracing::info!(
                "Processed {} blocks, found {} precommits",
                blocks_processed,
                precommits_found
            );
        }

        // Update health status after processing
        self.health_updater
            .update(Health::from(HealthStatus::Ready).with_details(*cursor));

        Ok(fetched_any)
    }
}
