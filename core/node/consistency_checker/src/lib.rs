use std::{borrow::Cow, collections::HashSet, fmt, time::Duration};

use anyhow::Context as _;
use serde::Serialize;
use tokio::sync::watch;
use zksync_contracts::PRE_BOOJUM_COMMIT_FUNCTION;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::{
    clients::{DynClient, L1},
    CallFunctionArgs, ContractCallError, EnrichedClientError, EthInterface,
};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_l1_contract_interface::{
    i_executor::{commit::kzg::ZK_SYNC_BYTES_PER_BLOB, structures::CommitBatchInfo},
    Tokenizable,
};
use zksync_shared_metrics::{CheckerComponent, EN_METRICS};
use zksync_types::{
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata},
    ethabi,
    ethabi::Token,
    pubdata_da::PubdataDA,
    Address, L1BatchNumber, ProtocolVersionId, H256, U256,
};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
enum CheckError {
    #[error("Web3 error communicating with L1")]
    Web3(#[from] EnrichedClientError),
    #[error("error calling L1 contract")]
    ContractCall(#[from] ContractCallError),
    /// Error that is caused by the main node providing incorrect information etc.
    #[error("failed validating commit transaction")]
    Validation(anyhow::Error),
    /// Error that is caused by violating invariants internal to *this* node (e.g., not having expected data in Postgres).
    #[error("internal error")]
    Internal(anyhow::Error),
}

impl CheckError {
    fn is_transient(&self) -> bool {
        match self {
            Self::Web3(err) | Self::ContractCall(ContractCallError::EthereumGateway(err)) => {
                err.is_transient()
            }
            _ => false,
        }
    }
}

/// Handler of life cycle events emitted by [`ConsistencyChecker`].
trait HandleConsistencyCheckerEvent: fmt::Debug + Send + Sync {
    fn initialize(&mut self);

    fn set_first_batch_to_check(&mut self, first_batch_to_check: L1BatchNumber);

    fn update_checked_batch(&mut self, last_checked_batch: L1BatchNumber);

    fn report_inconsistent_batch(&mut self, number: L1BatchNumber, err: &anyhow::Error);
}

/// Health details reported by [`ConsistencyChecker`].
#[derive(Debug, Default, Serialize)]
struct ConsistencyCheckerDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    first_checked_batch: Option<L1BatchNumber>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_checked_batch: Option<L1BatchNumber>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    inconsistent_batches: Vec<L1BatchNumber>,
}

impl ConsistencyCheckerDetails {
    fn health(&self) -> Health {
        let status = if self.inconsistent_batches.is_empty() {
            HealthStatus::Ready
        } else {
            HealthStatus::Affected
        };
        Health::from(status).with_details(self)
    }
}

/// Default [`HandleConsistencyCheckerEvent`] implementation that reports the batch number as a metric and via health check details.
#[derive(Debug)]
struct ConsistencyCheckerHealthUpdater {
    inner: HealthUpdater,
    current_details: ConsistencyCheckerDetails,
}

impl ConsistencyCheckerHealthUpdater {
    fn new() -> (ReactiveHealthCheck, Self) {
        let (health_check, health_updater) = ReactiveHealthCheck::new("consistency_checker");
        let this = Self {
            inner: health_updater,
            current_details: ConsistencyCheckerDetails::default(),
        };
        (health_check, this)
    }
}

impl HandleConsistencyCheckerEvent for ConsistencyCheckerHealthUpdater {
    fn initialize(&mut self) {
        self.inner.update(self.current_details.health());
    }

    fn set_first_batch_to_check(&mut self, first_batch_to_check: L1BatchNumber) {
        self.current_details.first_checked_batch = Some(first_batch_to_check);
        self.inner.update(self.current_details.health());
    }

    fn update_checked_batch(&mut self, last_checked_batch: L1BatchNumber) {
        tracing::info!("L1 batch #{last_checked_batch} is consistent with L1");
        EN_METRICS.last_correct_batch[&CheckerComponent::ConsistencyChecker]
            .set(last_checked_batch.0.into());
        self.current_details.last_checked_batch = Some(last_checked_batch);
        self.inner.update(self.current_details.health());
    }

    fn report_inconsistent_batch(&mut self, number: L1BatchNumber, err: &anyhow::Error) {
        tracing::warn!("L1 batch #{number} is inconsistent with L1: {err:?}");
        self.current_details.inconsistent_batches.push(number);
        self.inner.update(self.current_details.health());
    }
}

/// Consistency checker behavior when L1 commit data divergence is detected.
// This is a temporary workaround for a bug that sometimes leads to incorrect L1 batch data returned by the server
// (and thus persisted by external nodes). Eventually, we want to go back to bailing on L1 data mismatch;
// for now, it's only enabled for the unit tests.
#[derive(Debug)]
enum L1DataMismatchBehavior {
    #[cfg(test)]
    Bail,
    Log,
}

/// L1 commit data loaded from Postgres.
#[derive(Debug)]
struct LocalL1BatchCommitData {
    l1_batch: L1BatchWithMetadata,
    commit_tx_hash: H256,
    commitment_mode: L1BatchCommitmentMode,
}

impl LocalL1BatchCommitData {
    /// Returns `Ok(None)` if Postgres doesn't contain all data necessary to check L1 commitment
    /// for the specified batch.
    async fn new(
        storage: &mut Connection<'_, Core>,
        batch_number: L1BatchNumber,
        commitment_mode: L1BatchCommitmentMode,
    ) -> anyhow::Result<Option<Self>> {
        let Some(commit_tx_id) = storage
            .blocks_dal()
            .get_eth_commit_tx_id(batch_number)
            .await?
        else {
            return Ok(None);
        };

        let commit_tx_hash = storage
            .eth_sender_dal()
            .get_confirmed_tx_hash_by_eth_tx_id(commit_tx_id as u32)
            .await?
            .with_context(|| {
                format!("Commit tx hash not found in the database for tx id {commit_tx_id}")
            })?;

        let Some(l1_batch) = storage
            .blocks_dal()
            .get_l1_batch_metadata(batch_number)
            .await?
        else {
            return Ok(None);
        };

        let this = Self {
            l1_batch,
            commit_tx_hash,
            commitment_mode,
        };
        let metadata = &this.l1_batch.metadata;

        // For Boojum batches, `bootloader_initial_content_commitment` and `events_queue_commitment`
        // are computed by the commitment generator.
        // I.e., for these batches, we may have partial metadata in Postgres, which would not be sufficient
        // to compute local L1 commitment.
        if !this.is_pre_boojum()
            && (metadata.bootloader_initial_content_commitment.is_none()
                || metadata.events_queue_commitment.is_none())
        {
            return Ok(None);
        }

        Ok(Some(this))
    }

    fn is_pre_boojum(&self) -> bool {
        self.l1_batch
            .header
            .protocol_version
            .map_or(true, |version| version.is_pre_boojum())
    }

    fn is_pre_shared_bridge(&self) -> bool {
        self.l1_batch
            .header
            .protocol_version
            .map_or(true, |version| version.is_pre_shared_bridge())
    }

    /// All returned errors are validation errors.
    fn verify_commitment(&self, reference: &ethabi::Token) -> anyhow::Result<()> {
        let protocol_version = self
            .l1_batch
            .header
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);
        let da = detect_da(protocol_version, reference)
            .context("cannot detect DA source from reference commitment token")?;

        // For `PubdataDA::Calldata`, it's required that the pubdata fits into a single blob.
        if matches!(da, PubdataDA::Calldata) {
            let pubdata_len = self
                .l1_batch
                .header
                .pubdata_input
                .as_ref()
                .map_or_else(|| self.l1_batch.construct_pubdata().len(), Vec::len);
            anyhow::ensure!(
                pubdata_len <= ZK_SYNC_BYTES_PER_BLOB,
                "pubdata size is too large when using calldata DA source: expected <={ZK_SYNC_BYTES_PER_BLOB} bytes, \
                 got {pubdata_len} bytes"
            );
        }

        let local_token =
            CommitBatchInfo::new(self.commitment_mode, &self.l1_batch, da).into_token();
        anyhow::ensure!(
            local_token == *reference,
            "Locally reproduced commitment differs from the reference obtained from L1; \
             local: {local_token:?}, reference: {reference:?}"
        );
        Ok(())
    }
}

/// Determines which DA source was used in the `reference` commitment. It's assumed that the commitment was created
/// using `CommitBatchInfo::into_token()`.
///
/// # Errors
///
/// Returns an error if `reference` is malformed.
pub fn detect_da(
    protocol_version: ProtocolVersionId,
    reference: &Token,
) -> Result<PubdataDA, ethabi::Error> {
    /// These are used by the L1 Contracts to indicate what DA layer is used for pubdata
    const PUBDATA_SOURCE_CALLDATA: u8 = 0;
    const PUBDATA_SOURCE_BLOBS: u8 = 1;
    const PUBDATA_SOURCE_CUSTOM: u8 = 2;

    fn parse_error(message: impl Into<Cow<'static, str>>) -> ethabi::Error {
        ethabi::Error::Other(message.into())
    }

    if protocol_version.is_pre_1_4_2() {
        return Ok(PubdataDA::Calldata);
    }

    let reference = match reference {
        Token::Tuple(tuple) => tuple,
        _ => {
            return Err(parse_error(format!(
                "reference has unexpected shape; expected a tuple, got {reference:?}"
            )))
        }
    };
    let Some(last_reference_token) = reference.last() else {
        return Err(parse_error("reference commitment data is empty"));
    };

    let last_reference_token = match last_reference_token {
        Token::Bytes(bytes) => bytes,
        _ => return Err(parse_error(format!(
            "last reference token has unexpected shape; expected bytes, got {last_reference_token:?}"
        ))),
    };
    match last_reference_token.first() {
        Some(&byte) if byte == PUBDATA_SOURCE_CALLDATA => Ok(PubdataDA::Calldata),
        Some(&byte) if byte == PUBDATA_SOURCE_BLOBS => Ok(PubdataDA::Blobs),
        Some(&byte) if byte == PUBDATA_SOURCE_CUSTOM => Ok(PubdataDA::Custom),
        Some(&byte) => Err(parse_error(format!(
            "unexpected first byte of the last reference token; expected one of [{PUBDATA_SOURCE_CALLDATA}, {PUBDATA_SOURCE_BLOBS}], \
                got {byte}"
        ))),
        None => Err(parse_error("last reference token is empty")),
    }
}

#[derive(Debug)]
pub struct ConsistencyChecker {
    /// ABI of the ZKsync contract
    contract: ethabi::Contract,
    /// Address of the ZKsync diamond proxy on L1
    diamond_proxy_addr: Option<Address>,
    /// How many past batches to check when starting
    max_batches_to_recheck: u32,
    sleep_interval: Duration,
    l1_client: Box<DynClient<L1>>,
    migration_setup: Option<MigrationSetup>,
    event_handler: Box<dyn HandleConsistencyCheckerEvent>,
    l1_data_mismatch_behavior: L1DataMismatchBehavior,
    pool: ConnectionPool<Core>,
    health_check: ReactiveHealthCheck,
    commitment_mode: L1BatchCommitmentMode,
}

#[derive(Debug)]
pub struct MigrationSetup {
    pub client: Box<DynClient<L1>>,
    pub diamond_proxy_address: Option<Address>,
    pub first_batch_migrated: L1BatchNumber,
}

impl ConsistencyChecker {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub fn new(
        l1_client: Box<DynClient<L1>>,
        max_batches_to_recheck: u32,
        pool: ConnectionPool<Core>,
        commitment_mode: L1BatchCommitmentMode,
    ) -> anyhow::Result<Self> {
        let (health_check, health_updater) = ConsistencyCheckerHealthUpdater::new();
        Ok(Self {
            contract: zksync_contracts::hyperchain_contract(),
            diamond_proxy_addr: None,
            max_batches_to_recheck,
            sleep_interval: Self::DEFAULT_SLEEP_INTERVAL,
            l1_client: l1_client.for_component("consistency_checker"),
            migration_setup: None,
            event_handler: Box::new(health_updater),
            l1_data_mismatch_behavior: L1DataMismatchBehavior::Log,
            pool,
            health_check,
            commitment_mode,
        })
    }

    pub fn with_diamond_proxy_addr(mut self, address: Address) -> Self {
        self.diamond_proxy_addr = Some(address);
        self
    }

    pub fn with_migration_setup(
        mut self,
        client: Box<DynClient<L1>>,
        first_batch_migrated: L1BatchNumber,
        diamond_proxy_address: Option<Address>,
    ) -> Self {
        self.migration_setup = Some(MigrationSetup {
            client: client.for_component("consistency_checker"),
            diamond_proxy_address,
            first_batch_migrated,
        });
        self
    }

    /// Returns health check associated with this checker.
    pub fn health_check(&self) -> &ReactiveHealthCheck {
        &self.health_check
    }

    async fn check_commitments(
        &self,
        batch_number: L1BatchNumber,
        local: &LocalL1BatchCommitData,
    ) -> Result<(), CheckError> {
        let commit_tx_hash = local.commit_tx_hash;
        tracing::info!("Checking commit tx {commit_tx_hash} for L1 batch #{batch_number}");

        let client = match &self.migration_setup {
            Some(MigrationSetup {
                first_batch_migrated,
                client,
                ..
            }) if *first_batch_migrated <= local.l1_batch.header.number => client.as_ref(),
            _ => self.l1_client.as_ref(),
        };

        let commit_tx_status = client
            .get_tx_status(commit_tx_hash)
            .await?
            .with_context(|| format!("receipt for tx {commit_tx_hash:?} not found on L1"))
            .map_err(CheckError::Validation)?;
        if !commit_tx_status.success {
            let err = anyhow::anyhow!("main node gave us a failed commit tx {commit_tx_hash:?}");
            return Err(CheckError::Validation(err));
        }

        // We can't get tx calldata from the DB because it can be fake.
        let commit_tx = client
            .get_tx(commit_tx_hash)
            .await?
            .with_context(|| format!("commit transaction {commit_tx_hash:?} not found on L1"))
            .map_err(CheckError::Internal)?; // we've got a transaction receipt previously, thus an internal error

        if let Some(diamond_proxy_addr) = self.diamond_proxy_addr {
            let event = self
                .contract
                .event("BlockCommit")
                .context("`BlockCommit` event not found for ZKsync L1 contract")
                .map_err(CheckError::Internal)?;

            let committed_batch_numbers_by_logs =
                commit_tx_status.receipt.logs.into_iter().filter_map(|log| {
                    if log.address != diamond_proxy_addr {
                        return None;
                    }
                    let parsed_log = event
                        .parse_log_whole(ethabi::RawLog {
                            topics: log.topics,
                            data: log.data.0,
                        })
                        .ok()?;

                    parsed_log.params.into_iter().find_map(|param| {
                        (param.name == "batchNumber")
                            .then_some(param.value)
                            .and_then(ethabi::Token::into_uint)
                    })
                });
            let committed_batch_numbers_by_logs: HashSet<_> =
                committed_batch_numbers_by_logs.collect();
            tracing::debug!(
                "Commit transaction {commit_tx_hash:?} has `BlockCommit` event logs with the following batch numbers: \
                 {committed_batch_numbers_by_logs:?}"
            );

            if !committed_batch_numbers_by_logs.contains(&U256::from(batch_number.0)) {
                let err = anyhow::anyhow!(
                    "Commit transaction {commit_tx_hash:?} does not contain `BlockCommit` event log with batchNumber={batch_number}"
                );
                return Err(CheckError::Validation(err));
            }
        }

        let commit_function = if local.is_pre_boojum() {
            &*PRE_BOOJUM_COMMIT_FUNCTION
        } else if local.is_pre_shared_bridge() {
            self.contract
                .function("commitBatches")
                .context("L1 contract does not have `commitBatches` function")
                .map_err(CheckError::Internal)?
        } else {
            self.contract
                .function("commitBatchesSharedBridge")
                .context("L1 contract does not have `commitBatchesSharedBridge` function")
                .map_err(CheckError::Internal)?
        };

        let commitment =
            Self::extract_commit_data(&commit_tx.input.0, commit_function, batch_number)
                .with_context(|| {
                    format!("failed extracting commit data for transaction {commit_tx_hash:?}")
                })
                .map_err(CheckError::Validation)?;
        local
            .verify_commitment(&commitment)
            .map_err(CheckError::Validation)
    }

    /// All returned errors are validation errors.
    fn extract_commit_data(
        commit_tx_input_data: &[u8],
        commit_function: &ethabi::Function,
        batch_number: L1BatchNumber,
    ) -> anyhow::Result<ethabi::Token> {
        let expected_solidity_selector = commit_function.short_signature();
        let actual_solidity_selector = &commit_tx_input_data[..4];
        anyhow::ensure!(
            expected_solidity_selector == actual_solidity_selector,
            "unexpected Solidity function selector: expected {expected_solidity_selector:?}, got {actual_solidity_selector:?}"
        );

        let mut commit_input_tokens = commit_function
            .decode_input(&commit_tx_input_data[4..])
            .context("Failed decoding calldata for L1 commit function")?;
        let mut commitments = commit_input_tokens
            .pop()
            .context("Unexpected signature for L1 commit function")?
            .into_array()
            .context("Unexpected signature for L1 commit function")?;

        // Commit transactions usually publish multiple commitments at once, so we need to find
        // the one that corresponds to the batch we're checking.
        let first_batch_commitment = commitments
            .first()
            .context("L1 batch commitment is empty")?;
        let ethabi::Token::Tuple(first_batch_commitment) = first_batch_commitment else {
            anyhow::bail!("Unexpected signature for L1 commit function");
        };
        let first_batch_number = first_batch_commitment
            .first()
            .context("Unexpected signature for L1 commit function")?;
        let first_batch_number = first_batch_number
            .clone()
            .into_uint()
            .context("Unexpected signature for L1 commit function")?;
        let first_batch_number = usize::try_from(first_batch_number)
            .map_err(|_| anyhow::anyhow!("Integer overflow for L1 batch number"))?;
        // ^ `TryFrom` has `&str` error here, so we can't use `.context()`.

        let commitment = (batch_number.0 as usize)
            .checked_sub(first_batch_number)
            .and_then(|offset| {
                (offset < commitments.len()).then(|| commitments.swap_remove(offset))
            });
        commitment.with_context(|| {
            let actual_range = first_batch_number..(first_batch_number + commitments.len());
            format!(
                "Malformed commitment data; it should commit to L1 batch #{batch_number}, \
                 but it actually commits to batches #{actual_range:?}"
            )
        })
    }

    async fn last_committed_batch(&self) -> anyhow::Result<Option<L1BatchNumber>> {
        Ok(self
            .pool
            .connection()
            .await?
            .blocks_dal()
            .get_number_of_last_l1_batch_committed_on_eth()
            .await?)
    }

    async fn sanity_check_diamond_proxy_addr(&self) -> Result<(), CheckError> {
        let regular = self
            .diamond_proxy_addr
            .map(|addr| (addr, self.l1_client.as_ref()));
        let migration = self.migration_setup.as_ref().and_then(|setup| {
            setup
                .diamond_proxy_address
                .map(|addr| (addr, setup.client.as_ref()))
        });

        if regular.is_none() && migration.is_none() {
            return Ok(());
        }

        let (address, client) = regular.or(migration).unwrap();

        tracing::debug!("Performing sanity checks for diamond proxy contract {address:?}");
        let version: U256 = CallFunctionArgs::new("getProtocolVersion", ())
            .for_contract(address, &self.contract)
            .call(client)
            .await?;
        tracing::info!("Checked diamond proxy {address:?} (protocol version: {version})");
        Ok(())
    }

    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!(
            "Starting consistency checker with diamond proxy contract: {:?}, sleep interval: {:?}, \
             max historic L1 batches to check: {}",
            self.diamond_proxy_addr,
            self.sleep_interval,
            self.max_batches_to_recheck
        );
        self.event_handler.initialize();

        while let Err(err) = self.sanity_check_diamond_proxy_addr().await {
            if err.is_transient() {
                tracing::warn!(
                    "Transient error checking diamond proxy contract; will retry after a delay: {:#}",
                    anyhow::Error::from(err)
                );
                if tokio::time::timeout(self.sleep_interval, stop_receiver.changed())
                    .await
                    .is_ok()
                {
                    tracing::info!("Stop signal received, consistency_checker is shutting down");
                    return Ok(());
                }
            } else {
                return Err(anyhow::Error::from(err)
                    .context("failed sanity-checking diamond proxy contract"));
            }
        }

        // It doesn't make sense to start the checker until we have at least one L1 batch with metadata.
        let earliest_l1_batch_number =
            wait_for_l1_batch_with_metadata(&self.pool, self.sleep_interval, &mut stop_receiver)
                .await?;

        let Some(earliest_l1_batch_number) = earliest_l1_batch_number else {
            return Ok(()); // Stop signal received
        };

        let last_committed_batch = self
            .last_committed_batch()
            .await?
            .unwrap_or(earliest_l1_batch_number);
        let first_batch_to_check: L1BatchNumber = last_committed_batch
            .0
            .saturating_sub(self.max_batches_to_recheck)
            .into();

        let last_processed_batch = self
            .pool
            .connection()
            .await?
            .blocks_dal()
            .get_consistency_checker_last_processed_l1_batch()
            .await?;

        // We shouldn't check batches not present in the storage, and skip the genesis batch since
        // it's not committed on L1.
        let first_batch_to_check = first_batch_to_check
            .max(earliest_l1_batch_number)
            .max(L1BatchNumber(last_processed_batch.0 + 1));
        tracing::info!(
            "Last committed L1 batch is #{last_committed_batch}; starting checks from L1 batch #{first_batch_to_check}"
        );
        self.event_handler
            .set_first_batch_to_check(first_batch_to_check);

        let mut batch_number = first_batch_to_check;
        while !*stop_receiver.borrow_and_update() {
            let mut storage = self.pool.connection().await?;
            // The batch might be already committed but not yet processed by the external node's tree
            // OR the batch might be processed by the external node's tree but not yet committed.
            // We need both.
            let local =
                LocalL1BatchCommitData::new(&mut storage, batch_number, self.commitment_mode)
                    .await?;
            let Some(local) = local else {
                if tokio::time::timeout(self.sleep_interval, stop_receiver.changed())
                    .await
                    .is_ok()
                {
                    break;
                }
                continue;
            };
            drop(storage);

            match self.check_commitments(batch_number, &local).await {
                Ok(()) => {
                    let mut storage = self.pool.connection().await?;
                    storage
                        .blocks_dal()
                        .set_consistency_checker_last_processed_l1_batch(batch_number)
                        .await?;
                    self.event_handler.update_checked_batch(batch_number);
                    batch_number += 1;
                }
                Err(CheckError::Validation(err)) => {
                    self.event_handler
                        .report_inconsistent_batch(batch_number, &err);
                    match &self.l1_data_mismatch_behavior {
                        #[cfg(test)]
                        L1DataMismatchBehavior::Bail => {
                            let context =
                                format!("L1 batch #{batch_number} is inconsistent with L1");
                            return Err(err.context(context));
                        }
                        L1DataMismatchBehavior::Log => {
                            batch_number += 1; // We don't want to infinitely loop failing the check on the same batch
                        }
                    }
                }
                Err(err) if err.is_transient() => {
                    tracing::warn!(
                        "Transient error while verifying L1 batch #{batch_number}; will retry after a delay: {:#}",
                        anyhow::Error::from(err)
                    );
                    if tokio::time::timeout(self.sleep_interval, stop_receiver.changed())
                        .await
                        .is_ok()
                    {
                        break;
                    }
                }
                Err(other_err) => {
                    let context =
                        format!("failed verifying consistency of L1 batch #{batch_number}");
                    return Err(anyhow::Error::from(other_err).context(context));
                }
            }
        }

        tracing::info!("Stop signal received, consistency_checker is shutting down");
        Ok(())
    }
}

/// Repeatedly polls the DB until there is an L1 batch with metadata. We may not have such a batch initially
/// if the DB is recovered from an application-level snapshot.
///
/// Returns the number of the *earliest* L1 batch with metadata, or `None` if the stop signal is received.
async fn wait_for_l1_batch_with_metadata(
    pool: &ConnectionPool<Core>,
    poll_interval: Duration,
    stop_receiver: &mut watch::Receiver<bool>,
) -> anyhow::Result<Option<L1BatchNumber>> {
    loop {
        if *stop_receiver.borrow() {
            return Ok(None);
        }

        let mut storage = pool.connection().await?;
        let sealed_l1_batch_number = storage
            .blocks_dal()
            .get_earliest_l1_batch_number_with_metadata()
            .await?;
        drop(storage);

        if let Some(number) = sealed_l1_batch_number {
            return Ok(Some(number));
        }
        tracing::debug!(
            "No L1 batches with metadata are present in DB; trying again in {poll_interval:?}"
        );
        tokio::time::timeout(poll_interval, stop_receiver.changed())
            .await
            .ok();
    }
}
