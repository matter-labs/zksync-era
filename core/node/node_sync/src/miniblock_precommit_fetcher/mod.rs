//! Component responsible for fetching miniblock precommits.

use std::{fmt, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::EthInterface;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_types::{
    web3::{BlockId, BlockNumber},
    L2BlockNumber, SLChainId, H256,
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

#[derive(Debug, Clone)]
pub struct MiniblockPrecommitDetails {
    hash: H256,
    chain_id: SLChainId,
}

#[async_trait]
pub trait MainNodeClient: fmt::Debug + Send + Sync {
    async fn get_miniblock_precommit_hash(
        &self,
        number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<MiniblockPrecommitDetails>>;

    async fn get_safe_block(&self) -> EnrichedClientResult<L2BlockNumber>;
}

#[async_trait]
impl MainNodeClient for Box<DynClient<L2>> {
    async fn get_miniblock_precommit_hash(
        &self,
        number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<MiniblockPrecommitDetails>> {
        let request_latency = FETCHER_METRICS.requests[&FetchStage::GetMiniblockDetails].start();
        let details = self
            .get_block_details(number)
            .rpc_context("get_block_details")
            .with_arg("number", &number)
            .await?;
        request_latency.observe();
        Ok(details.and_then(|details| {
            if let (Some(precommit_tx_hash), Some(precommit_chain_id)) = (
                details.base.precommit_tx_hash,
                details.base.precommit_chain_id,
            ) {
                Some(MiniblockPrecommitDetails {
                    hash: precommit_tx_hash,
                    chain_id: precommit_chain_id,
                })
            } else {
                None
            }
        }))
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

    pub fn new(client: Box<dyn MainNodeClient>, pool: ConnectionPool<Core>) -> Self {
        Self {
            client,
            pool,
            health_updater: ReactiveHealthCheck::new("miniblock_precommit_fetcher").1,
            sleep_interval: Self::DEFAULT_SLEEP_INTERVAL,
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

    async fn get_latest_miniblock_number(&self) -> Result<L2BlockNumber, FetcherError> {
        let miniblock_number = self
            .pool
            .connection_tagged("sync_layer")
            .await
            .context("Failed to get connection for miniblock_precommit_fetcher")?
            .blocks_dal()
            .get_last_sealed_l2_block_header()
            .await
            .context("Cannot get latest miniblock")?
            .map(|header| header.number)
            .unwrap_or(L2BlockNumber(0));
        Ok(miniblock_number)
    }

    /// Fetches precommits for blocks from the cursor up to the safe block
    /// Returns true if any precommits were fetched
    async fn fetch_precommits(&self, cursor: &mut FetcherCursor) -> Result<bool, FetcherError> {
        // Get the current safe block from the main node
        let safe_block = self.client.get_safe_block().await?;
        let last_synced_miniblock = self.get_latest_miniblock_number().await?;

        if safe_block > last_synced_miniblock {
            tracing::debug!(
                "Not all miniblocks up to safe block {} are synced, precommit fetch only up to synced block {}",
                safe_block,
                last_synced_miniblock
            );
        }
        let processing_to = safe_block.min(last_synced_miniblock);
        let processing_from = cursor.last_processed_miniblock;

        // Check if there are new blocks to process
        if processing_to <= cursor.last_processed_miniblock {
            return Ok(false);
        }

        tracing::debug!(
            "Processing blocks from {} to {}",
            processing_from.0,
            processing_to.0
        );

        let mut blocks_processed = 0;
        let mut precommits_found = 0;
        let mut fetched_any = false;

        // Process blocks from the next one after the last processed up to the calculated end block
        for current_block in processing_from.0 + 1..=processing_to.0 {
            let current_block = L2BlockNumber(current_block);
            let precommit = self
                .client
                .get_miniblock_precommit_hash(current_block)
                .await?;

            blocks_processed += 1;

            match precommit {
                Some(precommit) => {
                    tracing::info!(
                        "Registering precommitment for block {}, tx_hash: {}",
                        current_block,
                        precommit.hash
                    );

                    // Store the precommitment in the database
                    self.pool
                        .connection_tagged("sync_layer")
                        .await
                        .context("Failed to get connection for miniblock_precommit_fetcher")?
                        .eth_sender_dal()
                        .insert_pending_received_precommit_eth_tx(
                            current_block,
                            precommit.hash,
                            Some(precommit.chain_id),
                        )
                        .await?;

                    precommits_found += 1;
                    fetched_any = true;
                }
                None => {
                    tracing::debug!("No precommitment for block {}", current_block.0);
                }
            }
            // Even if precommitment was not found, it does not need to be synced
            // in the future as the current miniblock is reported as safe by main node
            cursor.last_processed_miniblock = current_block;
        }

        if blocks_processed > 0 {
            tracing::debug!(
                "Processed successfully {} blocks ({} -> {}), found {} precommits",
                blocks_processed,
                processing_from.0,
                processing_to.0,
                precommits_found
            );
        }

        // Update health status after processing
        self.health_updater
            .update(Health::from(HealthStatus::Ready).with_details(*cursor));

        Ok(fetched_any)
    }
}
