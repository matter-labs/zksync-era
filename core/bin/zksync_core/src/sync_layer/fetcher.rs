use std::time::Duration;

use zksync_types::{explorer_api::BlockDetails, L1BatchNumber, MiniblockNumber};
use zksync_web3_decl::{
    jsonrpsee::{
        core::{Error as RpcError, RpcResult},
        http_client::{HttpClient, HttpClientBuilder},
    },
    namespaces::{EthNamespaceClient, ZksNamespaceClient},
};

use crate::sync_layer::sync_action::{BatchStatusChange, SyncAction};

use super::sync_action::ActionQueue;

const DELAY_INTERVAL: Duration = Duration::from_millis(500);
const RECONNECT_INTERVAL: Duration = Duration::from_secs(5);

/// Structure responsible for fetching batches and miniblock data from the main node.
#[derive(Debug)]
pub struct MainNodeFetcher {
    main_node_url: String,
    client: HttpClient,
    current_l1_batch: L1BatchNumber,
    current_miniblock: MiniblockNumber,

    last_executed_l1_batch: L1BatchNumber,
    last_proven_l1_batch: L1BatchNumber,
    last_committed_l1_batch: L1BatchNumber,

    actions: ActionQueue,
}

impl MainNodeFetcher {
    pub fn new(
        main_node_url: &str,
        current_l1_batch: L1BatchNumber,
        current_miniblock: MiniblockNumber,
        last_executed_l1_batch: L1BatchNumber,
        last_proven_l1_batch: L1BatchNumber,
        last_committed_l1_batch: L1BatchNumber,
        actions: ActionQueue,
    ) -> Self {
        let client = Self::build_client(main_node_url);

        Self {
            main_node_url: main_node_url.into(),
            client,
            current_l1_batch,
            current_miniblock,

            last_executed_l1_batch,
            last_proven_l1_batch,
            last_committed_l1_batch,

            actions,
        }
    }

    fn build_client(main_node_url: &str) -> HttpClient {
        HttpClientBuilder::default()
            .build(main_node_url)
            .expect("Unable to create a main node client")
    }

    pub async fn run(mut self) {
        vlog::info!(
            "Starting the fetcher routine. Initial miniblock: {}, initial l1 batch: {}",
            self.current_miniblock,
            self.current_l1_batch
        );
        // Run the main routine and reconnect upon the network errors.
        loop {
            match self.run_inner().await {
                Ok(()) => unreachable!("Fetcher actor never exits"),
                Err(RpcError::Transport(err)) => {
                    vlog::warn!("Following transport error occurred: {}", err);
                    vlog::info!("Trying to reconnect");
                    self.reconnect().await;
                }
                Err(err) => {
                    panic!("Unexpected error in the fetcher: {}", err);
                }
            }
        }
    }

    async fn reconnect(&mut self) {
        loop {
            self.client = Self::build_client(&self.main_node_url);
            if self.client.chain_id().await.is_ok() {
                vlog::info!("Reconnected");
                break;
            }
            vlog::warn!(
                "Reconnect attempt unsuccessful. Next attempt would happen after a timeout"
            );
            std::thread::sleep(RECONNECT_INTERVAL);
        }
    }

    async fn run_inner(&mut self) -> RpcResult<()> {
        loop {
            let mut progressed = false;

            if self.actions.has_action_capacity() {
                progressed |= self.fetch_next_miniblock().await?;
            }
            if self.actions.has_status_change_capacity() {
                progressed |= self.update_batch_statuses().await?;
            }

            if !progressed {
                // We didn't fetch any updated on this iteration, so to prevent a busy loop we wait a bit.
                vlog::debug!("No updates to discover, waiting");
                std::thread::sleep(DELAY_INTERVAL);
            }
        }
    }

    /// Tries to fetch the next miniblock and insert it to the sync queue.
    /// Returns `true` if a miniblock was processed and `false` otherwise.
    async fn fetch_next_miniblock(&mut self) -> RpcResult<bool> {
        let Some(miniblock_header) = self
                .client
                .get_block_details(self.current_miniblock)
                .await?
            else {
                return Ok(false);
            };

        let mut new_actions = Vec::new();
        if miniblock_header.l1_batch_number != self.current_l1_batch {
            assert_eq!(
                miniblock_header.l1_batch_number,
                self.current_l1_batch.next(),
                "Unexpected batch number in the next received miniblock"
            );

            vlog::info!(
                "New batch: {}. Timestamp: {}",
                miniblock_header.l1_batch_number,
                miniblock_header.timestamp
            );

            new_actions.push(SyncAction::OpenBatch {
                number: miniblock_header.l1_batch_number,
                timestamp: miniblock_header.timestamp,
                l1_gas_price: miniblock_header.l1_gas_price,
                l2_fair_gas_price: miniblock_header.l2_fair_gas_price,
                base_system_contracts_hashes: miniblock_header.base_system_contracts_hashes,
            });

            self.current_l1_batch += 1;
        } else {
            // New batch implicitly means a new miniblock, so we only need to push the miniblock action
            // if it's not a new batch.
            new_actions.push(SyncAction::Miniblock {
                number: miniblock_header.number,
                timestamp: miniblock_header.timestamp,
            });
        }

        let miniblock_txs = self
            .client
            .get_raw_block_transactions(self.current_miniblock)
            .await?
            .into_iter()
            .map(|tx| SyncAction::Tx(Box::new(tx)));
        new_actions.extend(miniblock_txs);
        new_actions.push(SyncAction::SealMiniblock);

        // Check if this was the last miniblock in the batch.
        // If we will receive `None` here, it would mean that it's the currently open batch and it was not sealed
        // after the current miniblock.
        let is_last_miniblock_of_batch = self
            .client
            .get_miniblock_range(self.current_l1_batch)
            .await?
            .map(|(_, last)| last.as_u32() == miniblock_header.number.0)
            .unwrap_or(false);
        if is_last_miniblock_of_batch {
            new_actions.push(SyncAction::SealBatch);
        }

        vlog::info!("New miniblock: {}", miniblock_header.number);
        self.current_miniblock += 1;
        self.actions.push_actions(new_actions);
        Ok(true)
    }

    /// Goes through the already fetched batches trying to update their statuses.
    /// Returns `true` if at least one batch was updated, and `false` otherwise.
    async fn update_batch_statuses(&mut self) -> RpcResult<bool> {
        assert!(
            self.last_executed_l1_batch <= self.last_proven_l1_batch,
            "Incorrect local state: executed batch must be proven"
        );
        assert!(
            self.last_proven_l1_batch <= self.last_committed_l1_batch,
            "Incorrect local state: proven batch must be committed"
        );
        assert!(
            self.last_committed_l1_batch <= self.current_l1_batch,
            "Incorrect local state: unkonwn batch marked as committed"
        );

        let mut applied_updates = false;
        for batch in
            (self.last_executed_l1_batch.next().0..=self.current_l1_batch.0).map(L1BatchNumber)
        {
            // While we may receive `None` for the `self.current_l1_batch`, it's OK: open batch is guaranteed to not
            // be sent to L1.
            let Some((start_miniblock, _)) = self.client.get_miniblock_range(batch).await? else {
                return Ok(applied_updates);
            };
            // We could've used any miniblock from the range, all of them share the same info.
            let Some(batch_info) = self
                .client
                .get_block_details(MiniblockNumber(start_miniblock.as_u32()))
                .await?
            else {
                // We cannot recover from an external API inconsistency.
                panic!(
                    "Node API is inconsistent: miniblock {} was reported to be a part of {} L1batch, \
                    but API has no information about this miniblock", start_miniblock, batch
                );
            };

            applied_updates |= self.update_committed_batch(&batch_info);
            applied_updates |= self.update_proven_batch(&batch_info);
            applied_updates |= self.update_executed_batch(&batch_info);

            if batch_info.commit_tx_hash.is_none() {
                // No committed batches after this one.
                break;
            }
        }

        Ok(applied_updates)
    }

    /// Returns `true` if batch info was updated.
    fn update_committed_batch(&mut self, batch_info: &BlockDetails) -> bool {
        if batch_info.commit_tx_hash.is_some()
            && batch_info.l1_batch_number == self.last_committed_l1_batch.next()
        {
            assert!(
                batch_info.committed_at.is_some(),
                "Malformed API response: batch is committed, but has no commit timestamp"
            );
            self.actions.push_commit_status_change(BatchStatusChange {
                number: batch_info.l1_batch_number,
                l1_tx_hash: batch_info.commit_tx_hash.unwrap(),
                happened_at: batch_info.committed_at.unwrap(),
            });
            vlog::info!("Batch {}: committed", batch_info.l1_batch_number);
            self.last_committed_l1_batch += 1;
            true
        } else {
            false
        }
    }

    /// Returns `true` if batch info was updated.
    fn update_proven_batch(&mut self, batch_info: &BlockDetails) -> bool {
        if batch_info.prove_tx_hash.is_some()
            && batch_info.l1_batch_number == self.last_proven_l1_batch.next()
        {
            assert!(
                batch_info.proven_at.is_some(),
                "Malformed API response: batch is proven, but has no prove timestamp"
            );
            self.actions.push_prove_status_change(BatchStatusChange {
                number: batch_info.l1_batch_number,
                l1_tx_hash: batch_info.prove_tx_hash.unwrap(),
                happened_at: batch_info.proven_at.unwrap(),
            });
            vlog::info!("Batch {}: proven", batch_info.l1_batch_number);
            self.last_proven_l1_batch += 1;
            true
        } else {
            false
        }
    }

    /// Returns `true` if batch info was updated.
    fn update_executed_batch(&mut self, batch_info: &BlockDetails) -> bool {
        if batch_info.execute_tx_hash.is_some()
            && batch_info.l1_batch_number == self.last_executed_l1_batch.next()
        {
            assert!(
                batch_info.executed_at.is_some(),
                "Malformed API response: batch is executed, but has no execute timestamp"
            );
            self.actions.push_execute_status_change(BatchStatusChange {
                number: batch_info.l1_batch_number,
                l1_tx_hash: batch_info.execute_tx_hash.unwrap(),
                happened_at: batch_info.executed_at.unwrap(),
            });
            vlog::info!("Batch {}: executed", batch_info.l1_batch_number);
            self.last_executed_l1_batch += 1;
            true
        } else {
            false
        }
    }
}
