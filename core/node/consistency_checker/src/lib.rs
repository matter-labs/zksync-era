use std::{borrow::Cow, cmp::Ordering, collections::HashSet, fmt, time::Duration};

use anyhow::Context as _;
use serde::Serialize;
use tokio::sync::watch;
use zksync_contracts::{
    POST_BOOJUM_COMMIT_FUNCTION, POST_SHARED_BRIDGE_COMMIT_FUNCTION,
    POST_V26_GATEWAY_COMMIT_FUNCTION, PRE_BOOJUM_COMMIT_FUNCTION,
};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_eth_client::{CallFunctionArgs, ContractCallError, EnrichedClientError, EthInterface};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_l1_contract_interface::{
    i_executor::structures::{
        CommitBatchInfo, StoredBatchInfo, PUBDATA_SOURCE_BLOBS, PUBDATA_SOURCE_CALLDATA,
        PUBDATA_SOURCE_CUSTOM_PRE_GATEWAY, SUPPORTED_ENCODING_VERSION,
    },
    Tokenizable,
};
use zksync_shared_metrics::{CheckerComponent, EN_METRICS};
use zksync_types::{
    commitment::{L1BatchCommitmentMode, L1BatchWithMetadata},
    ethabi,
    ethabi::{ParamType, Token},
    pubdata_da::PubdataSendingMode,
    settlement::SettlementLayer,
    try_stoppable, Address, L1BatchNumber, OrStopped, ProtocolVersionId, SLChainId, H256, U256,
};

pub mod node;
#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
enum CheckError {
    #[error("Web3 error communicating with L1")]
    Web3(#[from] EnrichedClientError),
    #[error("error calling L1 contract")]
    ContractCall(#[from] ContractCallError),
    /// Error that is caused by the main node providing incorrect information etc.
    #[error("failed validating commit transaction: {0}")]
    Validation(anyhow::Error),
    /// Error that is caused by violating invariants internal to *this* node (e.g., not having expected data in Postgres).
    #[error("internal error: {0}")]
    Internal(anyhow::Error),
}

impl CheckError {
    fn is_retriable(&self) -> bool {
        match self {
            Self::Web3(err) | Self::ContractCall(ContractCallError::EthereumGateway(err)) => {
                err.is_retryable()
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

/// L1 commit data loaded from Postgres.
#[derive(Debug)]
struct LocalL1BatchCommitData {
    l1_batch: L1BatchWithMetadata,
    commit_tx_hash: H256,
    commit_chain_id: Option<SLChainId>,
    commitment_mode: L1BatchCommitmentMode,
}

impl LocalL1BatchCommitData {
    /// Returns `Ok(None)` if Postgres doesn't contain all data necessary to check L1 commitment
    /// for the specified batch.
    async fn new(
        storage: &mut Connection<'_, Core>,
        batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<Self>> {
        if storage
            .data_availability_dal()
            .l1_batch_missing_data_availability(batch_number)
            .await?
        {
            tracing::warn!(
                "L1 batch #{batch_number} is missing DA information, da_fetcher might be not started"
            );
            return Ok(None);
        }

        let Some(commit_tx_id) = storage
            .blocks_dal()
            .get_eth_commit_tx_id(batch_number)
            .await?
        else {
            return Ok(None);
        };

        let Some(pubdata_params) = storage
            .blocks_dal()
            .get_l1_batch_pubdata_params(batch_number)
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

        let commit_chain_id = storage
            .eth_sender_dal()
            .get_batch_commit_chain_id(batch_number)
            .await?;

        let this = Self {
            l1_batch,
            commit_tx_hash,
            commitment_mode: pubdata_params.pubdata_type.into(),
            commit_chain_id,
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
            .is_none_or(|version| version.is_pre_boojum())
    }

    fn is_pre_shared_bridge(&self) -> bool {
        self.l1_batch
            .header
            .protocol_version
            .is_none_or(|version| version.is_pre_shared_bridge())
    }

    fn is_pre_v26_gateway(&self) -> bool {
        self.l1_batch
            .header
            .protocol_version
            .is_none_or(|version| version.is_pre_gateway())
    }

    fn is_pre_v29_interop(&self) -> bool {
        self.l1_batch
            .header
            .protocol_version
            .is_none_or(|version| version.is_pre_interop_fast_blocks())
    }

    /// All returned errors are validation errors.
    fn verify_commitment(&self, reference: &ethabi::Token, is_gateway: bool) -> anyhow::Result<()> {
        let protocol_version = self
            .l1_batch
            .header
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);
        let da = detect_da(
            protocol_version,
            reference,
            self.commitment_mode,
            is_gateway,
        )
        .context("cannot detect DA source from reference commitment token")?;

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
    commitment_mode: L1BatchCommitmentMode,
    is_gateway: bool,
) -> Result<PubdataSendingMode, ethabi::Error> {
    fn parse_error(message: impl Into<Cow<'static, str>>) -> ethabi::Error {
        ethabi::Error::Other(message.into())
    }

    if protocol_version.is_pre_1_4_2() {
        return Ok(PubdataSendingMode::Calldata);
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

    if protocol_version.is_pre_gateway() {
        return match last_reference_token.first() {
            Some(&byte) if byte == PUBDATA_SOURCE_CALLDATA => Ok(PubdataSendingMode::Calldata),
            Some(&byte) if byte == PUBDATA_SOURCE_BLOBS => Ok(PubdataSendingMode::Blobs),
            Some(&byte) if byte == PUBDATA_SOURCE_CUSTOM_PRE_GATEWAY => Ok(PubdataSendingMode::Custom),
            Some(&byte) => Err(parse_error(format!(
                "unexpected first byte of the last reference token; expected one of [{PUBDATA_SOURCE_CALLDATA}, {PUBDATA_SOURCE_BLOBS}, {PUBDATA_SOURCE_CUSTOM_PRE_GATEWAY}], \
                    got {byte}"
            ))),
            None => Err(parse_error("last reference token is empty")),
        };
    }

    match commitment_mode {
        L1BatchCommitmentMode::Validium => {
            // `Calldata`, `RelayedL2Calldata` and `Blobs` are encoded exactly the same way,
            // token is just a `state_diff_hash` for them.
            // For `Custom` it's `state_diff_hash` followed by `da_inclusion_data`. We can't distinguish
            // between `Calldata`/`RelayedL2Calldata`/`Blobs`/`Custom` with empty `da_inclusion_data`,
            // but it's ok to just return a `Calldata` given they are all encoded the same.
            match last_reference_token.len().cmp(&32) {
                Ordering::Equal => Ok(PubdataSendingMode::Calldata),
                Ordering::Greater => Ok(PubdataSendingMode::Custom),
                Ordering::Less => Err(parse_error(
                    "unexpected last reference token len for post-gateway version validium",
                )),
            }
        }
        L1BatchCommitmentMode::Rollup => {
            // For rollup the format of this token (`operatorDAInput`) is:
            // 32 bytes - `state_diff_hash`
            // 32 bytes - hash of the full pubdata
            // 1 byte - number of blobs
            // 32 bytes for each blob - hashes of blobs
            // 1 byte - pubdata source
            // X bytes - blob/pubdata commitments

            let number_of_blobs = last_reference_token.get(64).copied().ok_or_else(|| {
                parse_error(format!(
                    "last reference token is too short; expected at least 65 bytes, got {}",
                    last_reference_token.len()
                ))
            })? as usize;

            match last_reference_token.get(65 + 32 * number_of_blobs) {
                Some(&byte) if byte == PUBDATA_SOURCE_CALLDATA => if is_gateway {
                    Ok(PubdataSendingMode::RelayedL2Calldata)
                } else {
                    Ok(PubdataSendingMode::Calldata)
                },
                Some(&byte) if byte == PUBDATA_SOURCE_BLOBS => Ok(PubdataSendingMode::Blobs),
                Some(&byte) => Err(parse_error(format!(
                    "unexpected first byte of the last reference token for rollup; expected one of [{PUBDATA_SOURCE_CALLDATA}, {PUBDATA_SOURCE_BLOBS}], \
                got {byte}"
                ))),
                None => Err(parse_error(format!("last reference token is too short; expected at least 65 bytes, got {}", last_reference_token.len()))),
            }
        }
    }
}

#[derive(Debug)]
pub struct SLChainAccess {
    client: Box<dyn EthInterface>,
    chain_id: SLChainId,
    diamond_proxy_addr: Option<Address>,
}

#[derive(Debug)]
pub struct ConsistencyChecker {
    /// ABI of the ZKsync contract
    contract: ethabi::Contract,
    /// How many past batches to check when starting
    max_batches_to_recheck: u32,
    sleep_interval: Duration,
    chain_data: SLChainAccess,
    settlement_layer: SettlementLayer,
    event_handler: Box<dyn HandleConsistencyCheckerEvent>,
    pool: ConnectionPool<Core>,
    health_check: ReactiveHealthCheck,
}

impl ConsistencyChecker {
    const DEFAULT_SLEEP_INTERVAL: Duration = Duration::from_secs(5);

    pub async fn new(
        sl_client: Box<dyn EthInterface>,
        max_batches_to_recheck: u32,
        pool: ConnectionPool<Core>,
        settlement_layer: SettlementLayer,
    ) -> anyhow::Result<Self> {
        let (health_check, health_updater) = ConsistencyCheckerHealthUpdater::new();
        let sl_chain_id = sl_client.fetch_chain_id().await?;
        let chain_data = SLChainAccess {
            client: sl_client,
            chain_id: sl_chain_id,
            diamond_proxy_addr: None,
        };

        Ok(Self {
            contract: zksync_contracts::hyperchain_contract(),
            max_batches_to_recheck,
            sleep_interval: Self::DEFAULT_SLEEP_INTERVAL,
            chain_data,
            settlement_layer,
            event_handler: Box::new(health_updater),
            pool,
            health_check,
        })
    }

    pub fn with_sl_diamond_proxy_addr(mut self, address: Address) -> Self {
        self.chain_data.diamond_proxy_addr = Some(address);
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

        let commit_tx_status = self
            .chain_data
            .client
            .get_tx_status(commit_tx_hash)
            .await?
            .with_context(|| {
                format!(
                    "receipt for tx {commit_tx_hash:?} not found on target chain with id {}",
                    self.chain_data.chain_id
                )
            })
            .map_err(CheckError::Validation)?;
        if !commit_tx_status.success {
            let err = anyhow::anyhow!("main node gave us a failed commit tx {commit_tx_hash:?}");
            return Err(CheckError::Validation(err));
        }

        // We can't get tx calldata from the DB because it can be fake.
        let commit_tx = self
            .chain_data
            .client
            .get_tx(commit_tx_hash)
            .await?
            .with_context(|| format!("commit transaction {commit_tx_hash:?} not found on L1"))
            .map_err(CheckError::Internal)?; // we've got a transaction receipt previously, thus an internal error

        if let Some(diamond_proxy_addr) = self.chain_data.diamond_proxy_addr {
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
            &*POST_BOOJUM_COMMIT_FUNCTION
        } else if local.is_pre_v26_gateway() {
            &*POST_SHARED_BRIDGE_COMMIT_FUNCTION
        } else if local.is_pre_v29_interop() {
            &*POST_V26_GATEWAY_COMMIT_FUNCTION
        } else {
            self.contract
                .function("commitBatchesSharedBridge")
                .context("L1 contract does not have `commitBatchesSharedBridge` function")
                .map_err(CheckError::Internal)?
        };

        let commitment = Self::extract_commit_data(
            &commit_tx.input.0,
            commit_function,
            batch_number,
            local.is_pre_v26_gateway(),
        )
        .with_context(|| {
            format!("failed extracting commit data for transaction {commit_tx_hash:?}")
        })
        .map_err(CheckError::Validation)?;

        let is_gateway = self.settlement_layer.is_gateway();
        local
            .verify_commitment(&commitment, is_gateway)
            .map_err(CheckError::Validation)
    }

    /// All returned errors are validation errors.
    fn extract_commit_data(
        commit_tx_input_data: &[u8],
        commit_function: &ethabi::Function,
        batch_number: L1BatchNumber,
        pre_gateway: bool,
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
        let mut commitments: Vec<Token>;
        if pre_gateway {
            commitments = commit_input_tokens
                .pop()
                .context("Unexpected signature for L1 commit function")?
                .into_array()
                .context("Unexpected signature for L1 commit function")?;
        } else {
            let commitments_popped = commit_input_tokens
                .pop()
                .context("Unexpected signature for L1 commit function: no tokens")?;
            let commitment_bytes = match commitments_popped {
                Token::Bytes(arr) => arr,
                _ => anyhow::bail!(
                    "Unexpected signature for L1 commit function: last token is not bytes"
                ),
            };
            let (version, encoded_data) = commitment_bytes.split_at(1);
            anyhow::ensure!(
                version[0] == SUPPORTED_ENCODING_VERSION,
                "Unexpected encoding version: {}",
                version[0]
            );
            let decoded_data = ethabi::decode(
                &[
                    StoredBatchInfo::schema(),
                    ParamType::Array(Box::new(CommitBatchInfo::post_gateway_schema())),
                ],
                encoded_data,
            )
            .context("Failed to decode commitData")?;
            if let Some(Token::Array(batch_commitments)) = &decoded_data.get(1) {
                // Now you have access to `stored_batch_info` and `l1_batches_to_commit`
                // Process them as needed
                commitments = batch_commitments.clone();
            } else {
                anyhow::bail!("Unexpected data format");
            }
        }

        // Commit transactions usually publish multiple commitments at once, so we need to find
        // the one that corresponds to the batch we're checking.
        let first_batch_commitment = commitments
            .first()
            .context("L1 batch commitment is empty")?;
        let ethabi::Token::Tuple(first_batch_commitment) = first_batch_commitment else {
            anyhow::bail!("Unexpected signature for L1 commit function 3");
        };
        let first_batch_number = first_batch_commitment
            .first()
            .context("Unexpected signature for L1 commit function 4")?;
        let first_batch_number = first_batch_number
            .clone()
            .into_uint()
            .context("Unexpected signature for L1 commit function  5")?;
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
            .get_number_of_last_l1_batch_committed_finailized_on_eth()
            .await?)
    }

    async fn sanity_check_diamond_proxy_addr(&self) -> Result<(), CheckError> {
        let Some(address) = self.chain_data.diamond_proxy_addr else {
            return Ok(());
        };
        let chain_id = self.chain_data.chain_id;
        tracing::debug!(
            "Performing sanity checks for chain id {chain_id}, diamond proxy contract {address:?}"
        );

        let version: U256 = CallFunctionArgs::new("getProtocolVersion", ())
            .for_contract(address, &self.contract)
            .call(self.chain_data.client.as_ref())
            .await?;
        tracing::info!(
            "Checked chain id {chain_id}, diamond proxy {address:?} (protocol version: {version})"
        );
        Ok(())
    }

    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!(
            "Starting consistency checker with  diamond proxy contract: {:?}, \
             sleep interval: {:?}, max historic L1 batches to check: {}",
            self.chain_data.diamond_proxy_addr,
            self.sleep_interval,
            self.max_batches_to_recheck
        );
        self.event_handler.initialize();

        while let Err(err) = self.sanity_check_diamond_proxy_addr().await {
            if err.is_retriable() {
                tracing::warn!(
                    "Transient error checking diamond proxy contract; will retry after a delay: {:#}",
                    anyhow::Error::from(err)
                );
                if tokio::time::timeout(self.sleep_interval, stop_receiver.changed())
                    .await
                    .is_ok()
                {
                    tracing::info!("Stop request received, consistency_checker is shutting down");
                    return Ok(());
                }
            } else {
                return Err(anyhow::Error::from(err)
                    .context("failed sanity-checking diamond proxy contract"));
            }
        }

        // It doesn't make sense to start the checker until we have at least one L1 batch with metadata.
        let earliest_l1_batch_number = try_stoppable!(
            wait_for_l1_batch_with_metadata(&self.pool, self.sleep_interval, &mut stop_receiver)
                .await
        );

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
            let local = LocalL1BatchCommitData::new(&mut storage, batch_number).await?;
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

            if let Some(commit_chain_id) = local.commit_chain_id {
                if commit_chain_id != self.chain_data.chain_id {
                    if batch_number < last_committed_batch {
                        // It's ok to skip check for old batches.
                        tracing::info!(
                            "Skip checking batch #{batch_number}, it was committed to chain with id {commit_chain_id} \
                            while node is configured to check on chain with id {}",
                            self.chain_data.chain_id
                        );
                        batch_number += 1;
                        continue;
                    } else {
                        // Chain migrated to different SL, throw error so it can restart and reload SL data.
                        anyhow::bail!(
                            "Batch #{batch_number} was committed to chain with id {commit_chain_id} \
                            while node is configured to check chain with id {}. Error is thrown so node can restart and reload SL data. \
                            If node doesn't make any progress after restart, then it's bug, please contact developers.",
                            self.chain_data.chain_id
                        );
                    }
                }
            }

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
                    let context = format!("L1 batch #{batch_number} is inconsistent with L1");
                    return Err(err.context(context));
                }
                Err(err) if err.is_retriable() => {
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

        tracing::info!("Stop request received, consistency_checker is shutting down");
        Ok(())
    }
}

/// Repeatedly polls the DB until there is an L1 batch with metadata. We may not have such a batch initially
/// if the DB is recovered from an application-level snapshot.
///
/// Returns the number of the *earliest* L1 batch with metadata, or `None` if a stop request is received.
async fn wait_for_l1_batch_with_metadata(
    pool: &ConnectionPool<Core>,
    poll_interval: Duration,
    stop_receiver: &mut watch::Receiver<bool>,
) -> Result<L1BatchNumber, OrStopped> {
    loop {
        if *stop_receiver.borrow() {
            return Err(OrStopped::Stopped);
        }

        let mut storage = pool.connection().await?;
        let sealed_l1_batch_number = storage
            .blocks_dal()
            .get_earliest_l1_batch_number_with_metadata()
            .await?;
        drop(storage);

        if let Some(number) = sealed_l1_batch_number {
            return Ok(number);
        }
        tracing::debug!(
            "No L1 batches with metadata are present in DB; trying again in {poll_interval:?}"
        );
        tokio::time::timeout(poll_interval, stop_receiver.changed())
            .await
            .ok();
    }
}
