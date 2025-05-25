//! Component responsible for updating L1 batch status.

use std::{fmt, time::Duration};

use anyhow::Context as _;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
#[cfg(test)]
use tokio::sync::mpsc;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::EthInterface;
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_shared_metrics::EN_METRICS;
use zksync_types::{
    aggregated_operations::L1BatchAggregatedActionType, api, ethabi, web3::TransactionReceipt,
    Address, L1BatchNumber, SLChainId, H256, U64,
};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::{ClientRpcContext, EnrichedClientError, EnrichedClientResult},
    namespaces::ZksNamespaceClient,
};

use super::metrics::{FetchStage, FETCHER_METRICS};

#[cfg(test)]
mod tests;

fn l1_batch_stage_to_action_str(stage: AggregatedActionType) -> &'static str {
    match stage {
        AggregatedActionType::Commit => "committed",
        AggregatedActionType::PublishProofOnchain => "proven",
        AggregatedActionType::Execute => "executed",
    }
}

/// Represents a change in the batch status.
/// It may be a batch being committed, proven or executed.
#[derive(Debug)]
struct BatchStatusChange {
    number: L1BatchNumber,
    l1_tx_hash: H256,
    happened_at: DateTime<Utc>,
    sl_chain_id: Option<SLChainId>,
}

#[derive(Debug, Default)]
struct StatusChanges {
    commit: Vec<BatchStatusChange>,
    prove: Vec<BatchStatusChange>,
    execute: Vec<BatchStatusChange>,
}

impl StatusChanges {
    /// Returns true if there are no status changes.
    fn is_empty(&self) -> bool {
        self.commit.is_empty() && self.prove.is_empty() && self.execute.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
enum UpdaterError {
    #[error("JSON-RPC error communicating with main node")]
    Web3(#[from] EnrichedClientError),
    #[error("Internal error")]
    Internal(#[from] anyhow::Error),
}

impl From<zksync_dal::DalError> for UpdaterError {
    fn from(err: zksync_dal::DalError) -> Self {
        Self::Internal(err.into())
    }
}

#[async_trait]
trait MainNodeClient: fmt::Debug + Send + Sync {
    async fn batch_details(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<api::L1BatchDetails>>;
}

#[async_trait]
impl MainNodeClient for Box<DynClient<L2>> {
    async fn batch_details(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<api::L1BatchDetails>> {
        let request_latency = FETCHER_METRICS.requests[&FetchStage::GetL1BatchDetails].start();
        let details = self
            .get_l1_batch_details(number)
            .rpc_context("block_details")
            .with_arg("number", &number)
            .await?;
        request_latency.observe();
        Ok(details)
    }
}

#[async_trait]
trait SLClient: fmt::Debug + Send + Sync {
    async fn tx_receipt(&self, tx_hash: H256) -> anyhow::Result<Option<TransactionReceipt>>;
}

#[async_trait]
impl SLClient for Box<dyn EthInterface> {
    async fn tx_receipt(&self, tx_hash: H256) -> anyhow::Result<Option<TransactionReceipt>> {
        self.tx_receipt(tx_hash).await
    }
}

/// Verifies the L1 transaction against the database and the SL.
#[derive(Debug)]
struct L1TransactionVerifier {
    sl_client: Box<dyn SLClient>,
    diamond_proxy_addr: Address,
    /// ABI of the ZKsync contract
    contract: ethabi::Contract,
    pool: ConnectionPool<Core>,
    sl_chain_id: SLChainId,
}

impl L1TransactionVerifier {
    pub fn new(
        sl_client: Box<dyn SLClient>,
        diamond_proxy_addr: Address,
        pool: ConnectionPool<Core>,
        sl_chain_id: SLChainId,
    ) -> Self {
        Self {
            sl_client,
            diamond_proxy_addr,
            contract: zksync_contracts::hyperchain_contract(),
            pool,
            sl_chain_id,
        }
    }

    async fn validate_commit_tx_against_db(
        &self,
        commit_tx_hash: H256,
        batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        let db_batch = self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .blocks_dal()
            .get_l1_batch_metadata(batch_number)
            .await?
            .ok_or(anyhow::anyhow!(
                "Batch {} is not found in the database",
                batch_number
            ))?;

        let receipt = self
            .sl_client
            .tx_receipt(commit_tx_hash)
            .await?
            .context("Failed to fetch commit transaction receipt from SL")?;
        if receipt.status != Some(U64::one()) {
            let err = anyhow::anyhow!(
                "Commit transaction {commit_tx_hash} for batch {} is not successful",
                batch_number
            );
            return Err(err);
        }

        let event = self
            .contract
            .event("BlockCommit")
            .context("`BlockCommit` event not found for ZKsync L1 contract")?;

        let commited_batch_info: Option<(H256, H256)> =
            receipt.logs.into_iter().filter_map(|log| {
                if log.address != self.diamond_proxy_addr {
                    tracing::debug!(
                        "Log address {} does not match diamond proxy address {}, skipping",
                        log.address,
                        self.diamond_proxy_addr
                    );
                    return None;
                }
                let parsed_log = event
                    .parse_log_whole(ethabi::RawLog {
                        topics: log.topics,
                        data: log.data.0,
                    })
                    .ok()?; // Skip logs that are of different event type

                let block_number_from_log = parsed_log.params.iter().find_map(|param| {
                    (param.name == "batchNumber")
                        .then_some(param.value.clone())
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|batch_number_from_log| {
                            u32::try_from(batch_number_from_log)
                                    .ok()
                                    .map(L1BatchNumber)
                        })
                }).expect("Missing expected `batchNumber` parameter in `BlockCommit` event log");

                if block_number_from_log != batch_number {
                    tracing::warn!(
                        "Commit transaction {commit_tx_hash:?} has `BlockCommit` event log with batchNumber={block_number_from_log}, \
                        but we are checking for batchNumber={batch_number}"
                    );
                    return None;
                }

                let batch_hash = parsed_log.params.iter().find_map(|param| {
                    (param.name == "batchHash")
                        .then_some(param.value.clone())
                        .and_then(ethabi::Token::into_fixed_bytes).map(|bytes| H256::from_slice(&bytes))
                }).expect("Missing expected `batchHash` parameter in `BlockCommit` event log");

                let commitment = parsed_log.params.into_iter().find_map(|param| {
                    (param.name == "commitment")
                        .then_some(param.value)
                        .and_then(ethabi::Token::into_fixed_bytes).map(|bytes| H256::from_slice(&bytes))
                }).expect("Missing expected `commitment` parameter in `BlockCommit` event log");

                tracing::debug!(
                    "Commit transaction {commit_tx_hash:?} has `BlockCommit` event log with batch_hash={batch_hash:?} and commitment={commitment:?}"
                );

                Some((batch_hash, commitment))
            }).next();

        if let Some((batch_hash, commitment)) = commited_batch_info {
            if db_batch.metadata.commitment != commitment {
                let err = anyhow::anyhow!(
                    "Commit transaction {commit_tx_hash} for batch {} has different commitment: expected {:?}, got {:?}",
                    batch_number,
                    db_batch.metadata.commitment,
                    commitment
                );
                return Err(err);
            }
            if db_batch.metadata.root_hash != batch_hash {
                let err = anyhow::anyhow!(
                    "Commit transaction {commit_tx_hash} for batch {} has different root hash: expected {:?}, got {:?}",
                    batch_number,
                    db_batch.metadata.root_hash,
                    batch_hash
                );
                return Err(err);
            }
            // OK verified successfully the commit transaction.
            tracing::debug!(
                "Commit transaction {commit_tx_hash} for batch {} verified successfully",
                batch_number
            );
            Ok(())
        } else {
            let err = anyhow::anyhow!(
                "Commit transaction {commit_tx_hash} for batch {} does not have `BlockCommit` event log",
                batch_number
            );
            Err(err)
        }
    }

    async fn validate_prove_tx(
        &self,
        prove_tx_hash: H256,
        batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        let receipt = self
            .sl_client
            .tx_receipt(prove_tx_hash)
            .await?
            .context("Failed to fetch prove transaction receipt from SL")?;
        if receipt.status != Some(U64::one()) {
            let err = anyhow::anyhow!(
                "Prove transaction {prove_tx_hash} for batch {} is not successful",
                batch_number
            );
            return Err(err);
        }

        let event = self
            .contract
            .event("BlocksVerification")
            .context("`BlocksVerification` event not found for ZKsync L1 contract")?;

        let proved_from_to: Option<(u32, u32)> =
            receipt.logs.into_iter().filter_map(|log| {
                if log.address != self.diamond_proxy_addr {
                    tracing::debug!(
                        "Log address {} does not match diamond proxy address {}, skipping",
                        log.address,
                        self.diamond_proxy_addr
                    );
                    return None;
                }
                let parsed_log = event
                    .parse_log_whole(ethabi::RawLog {
                        topics: log.topics,
                        data: log.data.0,
                    })
                    .ok()?; // Skip logs that are of different event type

                let block_number_from = parsed_log.params.iter().find_map(|param| {
                    (param.name == "previousLastVerifiedBatch")
                        .then_some(param.value.clone())
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|batch_number_from_log| {
                            u32::try_from(batch_number_from_log).ok()
                        })
                }).expect("Missing expected `previousLastVerifiedBatch` parameter in `BlocksVerification` event log");
                let block_number_to = parsed_log.params.iter().find_map(|param| {
                    (param.name == "currentLastVerifiedBatch")
                        .then_some(param.value.clone())
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|batch_number_to_log| {
                            u32::try_from(batch_number_to_log).ok()
                        })
                }).expect("Missing expected `currentLastVerifiedBatch` parameter in `BlocksVerification` event log");
                Some((
                    block_number_from,
                    block_number_to,
                ))
            }).next();
        if let Some((from, to)) = proved_from_to {
            if from >= batch_number.0 {
                let err = anyhow::anyhow!(
                    "Prove transaction {prove_tx_hash} for batch {} has invalid `from` value: expected < {}, got {}",
                    batch_number,
                    batch_number.0,
                    from
                );
                return Err(err);
            }
            if to < batch_number.0 {
                let err = anyhow::anyhow!(
                    "Prove transaction {prove_tx_hash} for batch {} has invalid `to` value: expected >= {}, got {}",
                    batch_number,
                    batch_number.0,
                    to
                );
                return Err(err);
            }
            // OK verified successfully the prove transaction.
            tracing::debug!(
                "Prove transaction {prove_tx_hash} for batch {} verified successfully",
                batch_number
            );
            Ok(())
        } else {
            let err = anyhow::anyhow!(
                "Prove transaction {prove_tx_hash} for batch {} does not have `BlocksVerification` event log",
                batch_number
            );
            Err(err)
        }
    }

    /// Validates the execute transaction against the database.
    async fn validate_execute_tx(
        &self,
        execute_tx_hash: H256,
        batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        let db_batch = self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .blocks_dal()
            .get_l1_batch_metadata(batch_number)
            .await?
            .ok_or(anyhow::anyhow!(
                "Batch {} is not found in the database",
                batch_number
            ))?;

        let receipt = self
            .sl_client
            .tx_receipt(execute_tx_hash)
            .await?
            .context("Failed to fetch execute transaction receipt from SL")?;
        if receipt.status != Some(U64::one()) {
            let err = anyhow::anyhow!(
                "Execute transaction {execute_tx_hash} for batch {} is not successful",
                batch_number
            );
            return Err(err);
        }

        let event = self
            .contract
            .event("BlockExecution")
            .context("`BlockExecution` event not found for ZKsync L1 contract")?;

        let commited_batch_info: Option<(H256, H256)> =
            receipt.logs.into_iter().filter_map(|log| {
                if log.address != self.diamond_proxy_addr {
                    tracing::debug!(
                        "Log address {} does not match diamond proxy address {}, skipping",
                        log.address,
                        self.diamond_proxy_addr
                    );
                    return None;
                }
                let parsed_log = event
                    .parse_log_whole(ethabi::RawLog {
                        topics: log.topics,
                        data: log.data.0,
                    })
                    .ok()?; // Skip logs that are of different event type

                let block_number_from_log = parsed_log.params.iter().find_map(|param| {
                    (param.name == "batchNumber")
                        .then_some(param.value.clone())
                        .and_then(ethabi::Token::into_uint)
                        .and_then(|batch_number_from_log| {
                            u32::try_from(batch_number_from_log)
                                    .ok()
                                    .map(L1BatchNumber)
                        })
                }).expect("Missing expected `batchNumber` parameter in `BlockExecution` event log");

                if block_number_from_log != batch_number {
                    tracing::debug!(
                        "Skipping event log batchNumber={block_number_from_log} for commit transaction {execute_tx_hash:?}. \
                        We are checking for batchNumber={batch_number}"
                    );
                    return None;
                }

                let batch_hash = parsed_log.params.iter().find_map(|param| {
                    (param.name == "batchHash")
                        .then_some(param.value.clone())
                        .and_then(ethabi::Token::into_fixed_bytes).map(|bytes| H256::from_slice(&bytes))
                }).expect("Missing expected `batchHash` parameter in `BlockExecution` event log");

                let commitment = parsed_log.params.into_iter().find_map(|param| {
                    (param.name == "commitment")
                        .then_some(param.value)
                        .and_then(ethabi::Token::into_fixed_bytes).map(|bytes| H256::from_slice(&bytes))
                }).expect("Missing expected `commitment` parameter in `BlockExecution` event log");

                tracing::debug!(
                    "Execute transaction {execute_tx_hash:?} has `BlockExecution` event log with batch_hash={batch_hash:?} and commitment={commitment:?}"
                );

                Some((batch_hash, commitment))
            }).next();

        if let Some((batch_hash, commitment)) = commited_batch_info {
            if db_batch.metadata.commitment != commitment {
                let err = anyhow::anyhow!(
                    "Execute transaction {execute_tx_hash} for batch {} has different commitment: expected {:?}, got {:?}",
                    batch_number,
                    db_batch.metadata.commitment,
                    commitment
                );
                return Err(err);
            }
            if db_batch.metadata.root_hash != batch_hash {
                let err = anyhow::anyhow!(
                    "Execute transaction {execute_tx_hash} for batch {} has different root hash: expected {:?}, got {:?}",
                    batch_number,
                    db_batch.metadata.root_hash,
                    batch_hash
                );
                return Err(err);
            }
            // OK verified successfully the execute transaction.
            tracing::debug!(
                "Execute transaction {execute_tx_hash} for batch {} verified successfully",
                batch_number
            );
            Ok(())
        } else {
            let err = anyhow::anyhow!(
                "Execute transaction {execute_tx_hash} for batch {} does not have the corresponding `BlockExecution` event log",
                batch_number
            );
            Err(err)
        }
    }
}

/// Cursors for the last executed / proven / committed L1 batch numbers.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
struct UpdaterCursor {
    last_executed_l1_batch: L1BatchNumber,
    last_proven_l1_batch: L1BatchNumber,
    last_committed_l1_batch: L1BatchNumber,
}

impl UpdaterCursor {
    async fn new(storage: &mut Connection<'_, Core>) -> anyhow::Result<Self> {
        let first_l1_batch_number = projected_first_l1_batch(storage).await?;
        // Use the snapshot L1 batch, or the genesis batch if we are not using a snapshot. Technically, the snapshot L1 batch
        // is not necessarily proven / executed yet, but since it and earlier batches are not stored, it serves
        // a natural lower boundary for the cursor.
        let starting_l1_batch_number = L1BatchNumber(first_l1_batch_number.saturating_sub(1));

        let last_executed_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_executed_on_eth()
            .await?
            .unwrap_or(starting_l1_batch_number);
        let last_proven_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_proven_on_eth()
            .await?
            .unwrap_or(starting_l1_batch_number);
        let last_committed_l1_batch = storage
            .blocks_dal()
            .get_number_of_last_l1_batch_committed_on_eth()
            .await?
            .unwrap_or(starting_l1_batch_number);
        Ok(Self {
            last_executed_l1_batch,
            last_proven_l1_batch,
            last_committed_l1_batch,
        })
    }

    /// Extracts tx hash, timestamp and chain id of the operation.
    async fn extract_and_verify_op_data(
        l1_transaction_verifier: &L1TransactionVerifier,
        batch_info: &api::L1BatchDetails,
        stage: L1BatchAggregatedActionType,
    ) -> anyhow::Result<(Option<H256>, Option<DateTime<Utc>>, Option<SLChainId>)> {
        match stage {
            L1BatchAggregatedActionType::Commit => {
                if let Some(commit_tx_hash) = batch_info.base.commit_tx_hash {
                    if batch_info.base.commit_chain_id != Some(l1_transaction_verifier.sl_chain_id)
                    {
                        return Err(anyhow::anyhow!(
                            "Commit transaction chain ID {:?} does not match SL chain ID {}",
                            batch_info.base.commit_chain_id,
                            l1_transaction_verifier.sl_chain_id
                        ));
                    }
                    // Validate the commit transaction against the database.
                    l1_transaction_verifier
                        .validate_commit_tx_against_db(commit_tx_hash, batch_info.number)
                        .await
                        .context("Failed to validate commit transaction against the database")?;
                    Ok((
                        batch_info.base.commit_tx_hash,
                        batch_info.base.committed_at,
                        batch_info.base.commit_chain_id,
                    ))
                } else {
                    Ok((None, None, None))
                }
            }
            L1BatchAggregatedActionType::PublishProofOnchain => {
                if let Some(prove_tx_hash) = batch_info.base.prove_tx_hash {
                    if batch_info.base.prove_chain_id != Some(l1_transaction_verifier.sl_chain_id) {
                        return Err(anyhow::anyhow!(
                            "Prove transaction chain ID {:?} does not match SL chain ID {}",
                            batch_info.base.prove_chain_id,
                            l1_transaction_verifier.sl_chain_id
                        ));
                    }
                    // Validate the prove transaction events.
                    l1_transaction_verifier
                        .validate_prove_tx(prove_tx_hash, batch_info.number)
                        .await
                        .context("Failed to validate prove transaction")?;
                    Ok((
                        batch_info.base.prove_tx_hash,
                        batch_info.base.proven_at,
                        batch_info.base.prove_chain_id,
                    ))
                } else {
                    Ok((None, None, None))
                }
            }
            L1BatchAggregatedActionType::Execute => {
                if let Some(execute_tx_hash) = batch_info.base.execute_tx_hash {
                    if batch_info.base.execute_chain_id != Some(l1_transaction_verifier.sl_chain_id)
                    {
                        return Err(anyhow::anyhow!(
                            "Execute transaction chain ID {:?} does not match SL chain ID {}",
                            batch_info.base.execute_chain_id,
                            l1_transaction_verifier.sl_chain_id
                        ));
                    }
                    // Validate the execute transaction events.
                    l1_transaction_verifier
                        .validate_execute_tx(execute_tx_hash, batch_info.number)
                        .await
                        .context("Failed to validate execute transaction")?;

                    Ok((
                        Some(execute_tx_hash),
                        batch_info.base.executed_at,
                        batch_info.base.execute_chain_id,
                    ))
                } else {
                    Ok((None, None, None))
                }
            }
        }
    }

    async fn update(
        &mut self,
        status_changes: &mut StatusChanges,
        batch_info: &api::L1BatchDetails,
        l1_transaction_verifier: &L1TransactionVerifier,
    ) -> anyhow::Result<()> {
        for stage in [
            AggregatedActionType::Commit,
            AggregatedActionType::PublishProofOnchain,
            AggregatedActionType::Execute,
        ] {
            self.update_stage(status_changes, batch_info, l1_transaction_verifier, stage)
                .await?;
        }
        Ok(())
    }

    async fn update_stage(
        &mut self,
        status_changes: &mut StatusChanges,
        batch_info: &api::L1BatchDetails,
        l1_transaction_verifier: &L1TransactionVerifier,
        stage: L1BatchAggregatedActionType,
    ) -> anyhow::Result<()> {
        let (l1_tx_hash, happened_at, sl_chain_id) =
            Self::extract_and_verify_op_data(l1_transaction_verifier, batch_info, stage).await?;
        let (last_l1_batch, changes_to_update) = match stage {
            AggregatedActionType::Commit => (
                &mut self.last_committed_l1_batch,
                &mut status_changes.commit,
            ),
            AggregatedActionType::PublishProofOnchain => {
                (&mut self.last_proven_l1_batch, &mut status_changes.prove)
            }
            AggregatedActionType::Execute => (
                &mut self.last_executed_l1_batch,
                &mut status_changes.execute,
            ),
        };

        // Check whether we have all data for the update.
        let Some(l1_tx_hash) = l1_tx_hash else {
            return Ok(());
        };
        if batch_info.number != last_l1_batch.next() {
            return Ok(());
        }

        let action_str = l1_batch_stage_to_action_str(stage);
        let happened_at = happened_at.with_context(|| {
            format!("Malformed API response: batch is {action_str}, but has no relevant timestamp")
        })?;
        changes_to_update.push(BatchStatusChange {
            number: batch_info.number,
            l1_tx_hash,
            happened_at,
            sl_chain_id,
        });
        tracing::info!("Batch {}: {action_str}", batch_info.number);
        FETCHER_METRICS.l1_batch[&stage.into()].set(batch_info.number.0.into());
        *last_l1_batch += 1;
        Ok(())
    }
}

/// Component responsible for fetching the batch status changes, i.e. one that monitors whether the
/// locally applied batch was committed, proven or executed on L1.
///
/// In essence, it keeps track of the last batch number per status, and periodically polls the main
/// node on these batches in order to see whewith_sl_chain_idther the status has changed. If some changes were picked up,
/// the module updates the database to mirror the state observable from the main node. This is required for other components
/// (e.g., the API server and the consistency checker) to function properly. E.g., the API server returns commit / prove / execute
/// L1 transaction information in `zks_getBlockDetails` and `zks_getL1BatchDetails` RPC methods.
#[derive(Debug)]
pub struct BatchStatusUpdater {
    main_node_client: Box<dyn MainNodeClient>,
    l1_transaction_verifier: L1TransactionVerifier,
    pool: ConnectionPool<Core>,
    health_updater: HealthUpdater,
    sleep_interval: Duration,
    /// Test-only sender of status changes each time they are produced and applied to the storage.
    #[cfg(test)]
    changes_sender: mpsc::UnboundedSender<StatusChanges>,
}

impl BatchStatusUpdater {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(
        client: Box<DynClient<L2>>,
        sl_client: Box<dyn EthInterface>,
        diamond_proxy_addr: Address,
        pool: ConnectionPool<Core>,
        sl_chain_id: SLChainId,
    ) -> Self {
        Self::from_parts(
            Box::new(client.for_component("batch_status_updater")),
            Box::new(sl_client),
            diamond_proxy_addr,
            pool,
            Self::DEFAULT_SLEEP_INTERVAL,
            sl_chain_id,
        )
    }

    fn from_parts(
        main_client: Box<dyn MainNodeClient>,
        sl_client: Box<dyn SLClient>,
        diamond_proxy_addr: Address,
        pool: ConnectionPool<Core>,
        sleep_interval: Duration,
        sl_chain_id: SLChainId,
    ) -> Self {
        Self {
            main_node_client: main_client,
            l1_transaction_verifier: L1TransactionVerifier::new(
                sl_client,
                diamond_proxy_addr,
                pool.clone(),
                sl_chain_id,
            ),
            pool,
            health_updater: ReactiveHealthCheck::new("batch_status_updater").1,
            sleep_interval,

            #[cfg(test)]
            changes_sender: mpsc::unbounded_channel().0,
        }
    }

    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut storage = self.pool.connection_tagged("sync_layer").await?;
        let mut cursor = UpdaterCursor::new(&mut storage).await?;
        drop(storage);
        tracing::info!("Initialized batch status updater cursor: {cursor:?}");
        self.health_updater
            .update(Health::from(HealthStatus::Ready).with_details(cursor));

        while !*stop_receiver.borrow_and_update() {
            // Status changes are created externally, so that even if we will receive a network error
            // while requesting the changes, we will be able to process what we already fetched.
            let mut status_changes = StatusChanges::default();
            // Note that we don't update `cursor` here (it is copied), but rather only in `apply_status_changes`.
            match self.get_status_changes(&mut status_changes, cursor).await {
                Ok(()) => { /* everything went smoothly */ }
                Err(UpdaterError::Web3(err)) => {
                    tracing::warn!("Failed to get status changes from the main node: {err}");
                }
                Err(UpdaterError::Internal(err)) => return Err(err),
            }

            if status_changes.is_empty() {
                if tokio::time::timeout(self.sleep_interval, stop_receiver.changed())
                    .await
                    .is_ok()
                {
                    break;
                }
            } else {
                self.apply_status_changes(&mut cursor, status_changes)
                    .await?;
                self.health_updater
                    .update(Health::from(HealthStatus::Ready).with_details(cursor));
            }
        }

        tracing::info!("Stop request received, exiting the batch status updater routine");
        Ok(())
    }

    /// Goes through the already fetched batches trying to update their statuses.
    ///
    /// Fetched changes are capped by the last locally applied batch number, so
    /// it's safe to assume that every status change can safely be applied (no status
    /// changes "from the future").
    async fn get_status_changes(
        &self,
        status_changes: &mut StatusChanges,
        mut cursor: UpdaterCursor,
    ) -> Result<(), UpdaterError> {
        let total_latency = EN_METRICS.update_batch_statuses.start();
        let Some(last_sealed_batch) = self
            .pool
            .connection_tagged("sync_layer")
            .await?
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await?
        else {
            return Ok(()); // No L1 batches in the storage yet; do nothing.
        };

        let mut batch = cursor.last_executed_l1_batch.next();
        // In this loop we try to progress on the batch statuses, utilizing the same request to the node to potentially
        // update all three statuses (e.g. if the node is still syncing), but also skipping the gaps in the statuses
        // (e.g. if the last executed batch is 10, but the last proven is 20, we don't need to check the batches 11-19).
        while batch <= last_sealed_batch {
            // batch details fetched from the main node are untrusted and should be checked with SL
            let Some(untrusted_batch_info) = self.main_node_client.batch_details(batch).await?
            else {
                // Batch is not ready yet
                return Ok(());
            };

            cursor
                .update(
                    status_changes,
                    &untrusted_batch_info,
                    &self.l1_transaction_verifier,
                )
                .await?;

            // Check whether we can skip a part of the range.
            if untrusted_batch_info.base.commit_tx_hash.is_none() {
                // No committed batches after this one.
                break;
            } else if untrusted_batch_info.base.prove_tx_hash.is_none()
                && batch < cursor.last_committed_l1_batch
            {
                // The interval between this batch and the last committed one is not proven.
                batch = cursor.last_committed_l1_batch.next();
            } else if untrusted_batch_info.base.executed_at.is_none()
                && batch < cursor.last_proven_l1_batch
            {
                // The interval between this batch and the last proven one is not executed.
                batch = cursor.last_proven_l1_batch.next();
            } else {
                batch += 1;
            }
        }

        total_latency.observe();
        Ok(())
    }

    /// Inserts the provided status changes into the database.
    /// The status changes are applied to the database by inserting bogus confirmed transactions (with
    /// some fields missing/substituted) only to satisfy API needs; this component doesn't expect the updated
    /// tables to be ever accessed by the `eth_sender` module.
    async fn apply_status_changes(
        &self,
        cursor: &mut UpdaterCursor,
        changes: StatusChanges,
    ) -> anyhow::Result<()> {
        let total_latency = EN_METRICS.batch_status_updater_loop_iteration.start();
        let mut connection = self.pool.connection_tagged("sync_layer").await?;
        let mut transaction = connection.start_transaction().await?;
        let last_sealed_batch = transaction
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await?
            .context("L1 batches disappeared from Postgres")?;

        for change in &changes.commit {
            tracing::info!(
                "Commit status change: number {}, hash {}, happened at {}, on chainID {:?}",
                change.number,
                change.l1_tx_hash,
                change.happened_at,
                change.sl_chain_id
            );
            anyhow::ensure!(
                change.number <= last_sealed_batch,
                "Incorrect update state: unknown batch marked as committed"
            );

            transaction
                .eth_sender_dal()
                .insert_bogus_confirmed_eth_tx(
                    change.number,
                    AggregatedActionType::Commit,
                    change.l1_tx_hash,
                    change.happened_at,
                    change.sl_chain_id,
                )
                .await?;
            cursor.last_committed_l1_batch = change.number;
        }

        for change in &changes.prove {
            tracing::info!(
                "Prove status change: number {}, hash {}, happened at {}, on chainID {:?}",
                change.number,
                change.l1_tx_hash,
                change.happened_at,
                change.sl_chain_id
            );
            anyhow::ensure!(
                change.number <= cursor.last_committed_l1_batch,
                "Incorrect update state: proven batch must be committed"
            );

            transaction
                .eth_sender_dal()
                .insert_bogus_confirmed_eth_tx(
                    change.number,
                    AggregatedActionType::PublishProofOnchain,
                    change.l1_tx_hash,
                    change.happened_at,
                    change.sl_chain_id,
                )
                .await?;
            cursor.last_proven_l1_batch = change.number;
        }

        for change in &changes.execute {
            tracing::info!(
                "Execute status change: number {}, hash {}, happened at {}, on chainID {:?}",
                change.number,
                change.l1_tx_hash,
                change.happened_at,
                change.sl_chain_id
            );
            anyhow::ensure!(
                change.number <= cursor.last_proven_l1_batch,
                "Incorrect update state: executed batch must be proven"
            );

            transaction
                .eth_sender_dal()
                .insert_bogus_confirmed_eth_tx(
                    change.number,
                    AggregatedActionType::Execute,
                    change.l1_tx_hash,
                    change.happened_at,
                    change.sl_chain_id,
                )
                .await?;
            cursor.last_executed_l1_batch = change.number;
        }
        transaction.commit().await?;
        total_latency.observe();

        #[cfg(test)]
        self.changes_sender.send(changes).ok();
        Ok(())
    }
}

/// Returns the projected number of the first locally available L1 batch. The L1 batch is **not**
/// guaranteed to be present in the storage!
async fn projected_first_l1_batch(
    storage: &mut Connection<'_, Core>,
) -> anyhow::Result<L1BatchNumber> {
    let snapshot_recovery = storage
        .snapshot_recovery_dal()
        .get_applied_snapshot_status()
        .await?;
    Ok(snapshot_recovery.map_or(L1BatchNumber(0), |recovery| recovery.l1_batch_number + 1))
}
