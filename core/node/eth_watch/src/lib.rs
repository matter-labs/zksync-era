//! Ethereum watcher polls the Ethereum node for the relevant events, such as priority operations (aka L1 transactions),
//! protocol upgrades etc.
//! New events are accepted to the ZKsync network once they have the sufficient amount of L1 confirmations.

use std::{sync::Arc, time::Duration};

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_config::ContractsConfig;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal, DalError};
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_system_constants::PRIORITY_EXPIRATION;
use zksync_types::{
    abi::ZkChainSpecificUpgradeData, ethabi::Contract, protocol_version::ProtocolSemanticVersion,
    tokens::TokenMetadata, web3::BlockNumber as Web3BlockNumber, L1BatchNumber, L2ChainId,
    PriorityOpId,
};

pub use self::client::{EthClient, EthHttpQueryClient, L2EthClient};
use self::{
    client::{L2EthClientW, RETRY_LIMIT},
    event_processors::{EventProcessor, EventProcessorError, PriorityOpsEventProcessor},
    metrics::METRICS,
};
use crate::event_processors::{
    BatchRootProcessor, DecentralizedUpgradesEventProcessor, EventsSource,
};

mod client;
mod event_processors;
mod metrics;
#[cfg(test)]
mod tests;

#[derive(Debug)]
struct EthWatchState {
    last_seen_protocol_version: ProtocolSemanticVersion,
    next_expected_priority_id: PriorityOpId,
    chain_batch_root_number_lower_bound: L1BatchNumber,
    batch_merkle_tree: MiniMerkleTree<[u8; 96]>,
}

/// Ethereum watcher component.
#[derive(Debug)]
pub struct EthWatch {
    l1_client: Arc<dyn EthClient>,
    sl_client: Arc<dyn EthClient>,
    poll_interval: Duration,
    event_processors: Vec<Box<dyn EventProcessor>>,
    pool: ConnectionPool<Core>,
}

impl EthWatch {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        chain_admin_contract: &Contract,
        l1_client: Box<dyn EthClient>,
        sl_l2_client: Option<Box<dyn L2EthClient>>,
        pool: ConnectionPool<Core>,
        poll_interval: Duration,
        contracts_config: &ContractsConfig,
        chain_id: L2ChainId,
    ) -> anyhow::Result<Self> {
        let mut storage = pool.connection_tagged("eth_watch").await?;
        let l1_client: Arc<dyn EthClient> = l1_client.into();
        let sl_l2_client: Option<Arc<dyn L2EthClient>> = sl_l2_client.map(Into::into);
        let sl_client: Arc<dyn EthClient> = if let Some(sl_l2_client) = sl_l2_client.clone() {
            Arc::new(L2EthClientW(sl_l2_client))
        } else {
            l1_client.clone()
        };

        let state = Self::initialize_state(&mut storage, sl_client.as_ref()).await?;
        tracing::info!("initialized state: {state:?}");
        drop(storage);

        let priority_ops_processor =
            PriorityOpsEventProcessor::new(state.next_expected_priority_id, sl_client.clone())?;
        let decentralized_upgrades_processor = DecentralizedUpgradesEventProcessor::new(
            state.last_seen_protocol_version,
            chain_admin_contract,
            get_chain_specific_upgrade_params(&l1_client, contracts_config).await?,
            sl_client.clone(),
            l1_client.clone(),
        );
        let mut event_processors: Vec<Box<dyn EventProcessor>> = vec![
            Box::new(priority_ops_processor),
            Box::new(decentralized_upgrades_processor),
        ];
        if let Some(sl_l2_client) = sl_l2_client {
            let batch_root_processor = BatchRootProcessor::new(
                state.chain_batch_root_number_lower_bound,
                state.batch_merkle_tree,
                chain_id,
                sl_l2_client,
            );
            event_processors.push(Box::new(batch_root_processor));
        }
        Ok(Self {
            l1_client,
            sl_client,
            poll_interval,
            event_processors,
            pool,
        })
    }

    #[tracing::instrument(name = "EthWatch::initialize_state", skip_all)]
    async fn initialize_state(
        storage: &mut Connection<'_, Core>,
        sl_client: &dyn EthClient,
    ) -> anyhow::Result<EthWatchState> {
        let next_expected_priority_id: PriorityOpId = storage
            .transactions_dal()
            .last_priority_id()
            .await?
            .map_or(PriorityOpId(0), |e| e + 1);

        let last_seen_protocol_version = storage
            .protocol_versions_dal()
            .latest_semantic_version()
            .await?
            .context("expected at least one (genesis) version to be present in DB")?;

        let sl_chain_id = sl_client.chain_id().await?;
        let batch_hashes = storage
            .blocks_dal()
            .get_executed_batch_roots_on_sl(sl_chain_id)
            .await?;

        let chain_batch_root_number_lower_bound = batch_hashes
            .last()
            .map(|(n, _)| *n + 1)
            .unwrap_or(L1BatchNumber(0));
        let tree_leaves = batch_hashes.into_iter().map(|(batch_number, batch_root)| {
            BatchRootProcessor::batch_leaf_preimage(batch_root, batch_number)
        });
        let batch_merkle_tree = MiniMerkleTree::new(tree_leaves, None);

        Ok(EthWatchState {
            next_expected_priority_id,
            last_seen_protocol_version,
            chain_batch_root_number_lower_bound,
            batch_merkle_tree,
        })
    }

    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut timer = tokio::time::interval(self.poll_interval);
        let pool = self.pool.clone();

        while !*stop_receiver.borrow_and_update() {
            tokio::select! {
                _ = timer.tick() => { /* continue iterations */ }
                _ = stop_receiver.changed() => break,
            }
            METRICS.eth_poll.inc();

            let mut storage = pool.connection_tagged("eth_watch").await?;
            match self.loop_iteration(&mut storage).await {
                Ok(()) => { /* everything went fine */ }
                Err(EventProcessorError::Internal(err)) => {
                    tracing::error!("Internal error processing new blocks: {err:?}");
                    return Err(err);
                }
                Err(err) => {
                    // This is an error because otherwise we could potentially miss a priority operation
                    // thus entering priority mode, which is not desired.
                    tracing::error!("Failed to process new blocks: {err}");
                }
            }
        }

        tracing::info!("Stop signal received, eth_watch is shutting down");
        Ok(())
    }

    #[tracing::instrument(name = "EthWatch::loop_iteration", skip_all)]
    async fn loop_iteration(
        &mut self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EventProcessorError> {
        for processor in &mut self.event_processors {
            let client = match processor.event_source() {
                EventsSource::L1 => self.l1_client.as_ref(),
                EventsSource::SL => self.sl_client.as_ref(),
            };
            let chain_id = client.chain_id().await?;
            let to_block = if processor.only_finalized_block() {
                client.finalized_block_number().await?
            } else {
                client.confirmed_block_number().await?
            };

            let from_block = storage
                .eth_watcher_dal()
                .get_or_set_next_block_to_process(
                    processor.event_type(),
                    chain_id,
                    to_block.saturating_sub(PRIORITY_EXPIRATION),
                )
                .await
                .map_err(DalError::generalize)?;

            // There are no new blocks so there is nothing to be done
            if from_block > to_block {
                continue;
            }

            let processor_events = client
                .get_events(
                    Web3BlockNumber::Number(from_block.into()),
                    Web3BlockNumber::Number(to_block.into()),
                    processor.topic1(),
                    processor.topic2(),
                    RETRY_LIMIT,
                )
                .await?;
            let processed_events_count = processor
                .process_events(storage, processor_events.clone())
                .await?;

            let next_block_to_process = if processed_events_count == processor_events.len() {
                to_block + 1
            } else if processed_events_count == 0 {
                //nothing was processed
                from_block
            } else {
                processor_events[processed_events_count - 1]
                    .block_number
                    .expect("Event block number is missing")
                    .try_into()
                    .unwrap()
            };

            storage
                .eth_watcher_dal()
                .update_next_block_to_process(
                    processor.event_type(),
                    chain_id,
                    next_block_to_process,
                )
                .await
                .map_err(DalError::generalize)?;
        }
        Ok(())
    }
}

async fn get_chain_specific_upgrade_params(
    l1_client: &Arc<dyn EthClient>,
    contracts_config: &ContractsConfig,
) -> anyhow::Result<Option<ZkChainSpecificUpgradeData>> {
    let TokenMetadata { name, symbol, .. } = l1_client.get_base_token_metadata().await?;

    Ok(ZkChainSpecificUpgradeData::from_partial_components(
        contracts_config.base_token_asset_id,
        contracts_config.l2_legacy_shared_bridge_addr,
        contracts_config.predeployed_l2_wrapped_base_token_address,
        contracts_config.base_token_addr,
        Some(name),
        Some(symbol),
    ))
}
