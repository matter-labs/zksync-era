use std::time::{Duration, Instant};

use anyhow::Context;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_multivm::{
    interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode},
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
    zk_evm_latest::ethereum_types::H256,
};
use zksync_types::{
    block::L2BlockHeader, commitment::PubdataParams, fee_model::BatchFeeInput,
    snapshots::SnapshotRecoveryStatus, Address, L1BatchNumber, L2BlockNumber, L2ChainId,
    ProtocolVersionId, ZKPORTER_IS_AVAILABLE,
};

/// Typesafe wrapper around [`L2BlockHeader`] returned by [`L1BatchParamsProvider`].
#[derive(Debug)]
pub struct FirstL2BlockInBatch {
    header: L2BlockHeader,
    l1_batch_number: L1BatchNumber,
}

impl FirstL2BlockInBatch {
    pub fn number(&self) -> L2BlockNumber {
        self.header.number
    }

    pub fn has_protocol_version(&self) -> bool {
        self.header.protocol_version.is_some()
    }

    pub fn set_protocol_version(&mut self, version: ProtocolVersionId) {
        assert!(
            self.header.protocol_version.is_none(),
            "Cannot redefine protocol version"
        );
        self.header.protocol_version = Some(version);
    }
}

/// Returns the parameters required to initialize the VM for the next L1 batch.
#[allow(clippy::too_many_arguments)]
pub fn l1_batch_params(
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    l1_batch_timestamp: u64,
    previous_batch_hash: H256,
    fee_input: BatchFeeInput,
    first_l2_block_number: L2BlockNumber,
    prev_l2_block_hash: H256,
    base_system_contracts: BaseSystemContracts,
    validation_computational_gas_limit: u32,
    protocol_version: ProtocolVersionId,
    virtual_blocks: u32,
    chain_id: L2ChainId,
    pubdata_params: PubdataParams,
) -> (SystemEnv, L1BatchEnv) {
    (
        SystemEnv {
            zk_porter_available: ZKPORTER_IS_AVAILABLE,
            version: protocol_version,
            base_system_smart_contracts: base_system_contracts,
            bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
            execution_mode: TxExecutionMode::VerifyExecute,
            default_validation_computational_gas_limit: validation_computational_gas_limit,
            chain_id,
            pubdata_params,
        },
        L1BatchEnv {
            previous_batch_hash: Some(previous_batch_hash),
            number: current_l1_batch_number,
            timestamp: l1_batch_timestamp,
            fee_input,
            fee_account,
            enforced_base_fee: None,
            first_l2_block: L2BlockEnv {
                number: first_l2_block_number.0,
                timestamp: l1_batch_timestamp,
                prev_block_hash: prev_l2_block_hash,
                max_virtual_blocks_to_create: virtual_blocks,
            },
        },
    )
}

/// Provider of L1 batch parameters for state keeper I/O implementations. The provider is stateless; i.e., it doesn't
/// enforce a particular order of method calls.
#[derive(Debug, Default)]
pub struct L1BatchParamsProvider {
    snapshot: Option<SnapshotRecoveryStatus>,
}

impl L1BatchParamsProvider {
    pub fn new() -> Self {
        Self { snapshot: None }
    }

    /// Performs the provider initialization. Must only be called with the initialized storage (e.g.
    /// either after genesis or snapshot recovery).
    pub async fn initialize(&mut self, storage: &mut Connection<'_, Core>) -> anyhow::Result<()> {
        if storage
            .blocks_dal()
            .get_earliest_l1_batch_number()
            .await?
            .is_some()
        {
            // We have batches in the storage, no need for special treatment.
            return Ok(());
        }

        let Some(snapshot) = storage
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await
            .context("failed getting snapshot recovery status")?
        else {
            anyhow::bail!(
                "Storage is not initialized, it doesn't have batches or snapshot recovery status"
            )
        };
        self.snapshot = Some(snapshot);
        Ok(())
    }

    /// Returns state root hash and timestamp of an L1 batch with the specified number waiting for the hash to be computed
    /// if necessary.
    pub async fn wait_for_l1_batch_params(
        &self,
        storage: &mut Connection<'_, Core>,
        number: L1BatchNumber,
    ) -> anyhow::Result<(H256, u64)> {
        let first_l1_batch = if let Some(snapshot) = &self.snapshot {
            // Special case: if we've recovered from a snapshot, we allow to wait for the snapshot L1 batch.
            if number == snapshot.l1_batch_number {
                return Ok((snapshot.l1_batch_root_hash, snapshot.l1_batch_timestamp));
            }
            snapshot.l1_batch_number + 1
        } else {
            L1BatchNumber(0)
        };

        anyhow::ensure!(
            number >= first_l1_batch,
            "Cannot wait a hash of a pruned L1 batch #{number} (first retained batch: {first_l1_batch})"
        );
        Self::wait_for_l1_batch_params_unchecked(storage, number).await
    }

    async fn wait_for_l1_batch_params_unchecked(
        storage: &mut Connection<'_, Core>,
        number: L1BatchNumber,
    ) -> anyhow::Result<(H256, u64)> {
        // If the state root is not known yet, this duration will be used to back off in the while loops
        const SAFE_STATE_ROOT_INTERVAL: Duration = Duration::from_millis(100);

        let stage_started_at: Instant = Instant::now();
        loop {
            let data = storage
                .blocks_dal()
                .get_l1_batch_state_root_and_timestamp(number)
                .await?;
            if let Some((root_hash, timestamp)) = data {
                tracing::trace!(
                    "Waiting for hash of L1 batch #{number} took {:?}",
                    stage_started_at.elapsed()
                );
                return Ok((root_hash, timestamp));
            }

            tokio::time::sleep(SAFE_STATE_ROOT_INTERVAL).await;
        }
    }

    pub async fn load_l1_batch_protocol_version(
        &self,
        storage: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<ProtocolVersionId>> {
        if let Some(snapshot) = &self.snapshot {
            if l1_batch_number == snapshot.l1_batch_number {
                return Ok(Some(snapshot.protocol_version));
            }
            anyhow::ensure!(
                l1_batch_number > snapshot.l1_batch_number,
                "Requested protocol version for pruned L1 batch #{l1_batch_number}; first retained batch is #{}",
                snapshot.l1_batch_number + 1
            );
        }

        storage
            .blocks_dal()
            .get_batch_protocol_version_id(l1_batch_number)
            .await
            .map_err(Into::into)
    }

    /// Returns a header of the first L2 block in the specified L1 batch regardless of whether the batch is sealed or not.
    pub async fn load_first_l2_block_in_batch(
        &self,
        storage: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<FirstL2BlockInBatch>> {
        let l2_block_number = self
            .load_number_of_first_l2_block_in_batch(storage, l1_batch_number)
            .await
            .context("failed getting first L2 block number")?;
        Ok(match l2_block_number {
            Some(number) => storage
                .blocks_dal()
                .get_l2_block_header(number)
                .await?
                .map(|header| FirstL2BlockInBatch {
                    header,
                    l1_batch_number,
                }),
            None => None,
        })
    }

    #[doc(hidden)] // public for testing purposes
    pub async fn load_number_of_first_l2_block_in_batch(
        &self,
        storage: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<L2BlockNumber>> {
        if l1_batch_number == L1BatchNumber(0) {
            return Ok(Some(L2BlockNumber(0)));
        }

        if let Some(snapshot) = &self.snapshot {
            anyhow::ensure!(
                l1_batch_number > snapshot.l1_batch_number,
                "Cannot load L2 blocks for pruned L1 batch #{l1_batch_number} (first retained batch: {})",
                snapshot.l1_batch_number + 1
            );
            if l1_batch_number == snapshot.l1_batch_number + 1 {
                return Ok(Some(snapshot.l2_block_number + 1));
            }
        }

        let prev_l1_batch = l1_batch_number - 1;
        // At this point, we have ensured that `prev_l1_batch` is not pruned.
        let Some((_, last_l2_block_in_prev_l1_batch)) = storage
            .blocks_dal()
            .get_l2_block_range_of_l1_batch(prev_l1_batch)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(last_l2_block_in_prev_l1_batch + 1))
    }

    /// Loads VM-related L1 batch parameters for the specified batch.
    pub async fn load_l1_batch_params(
        &self,
        storage: &mut Connection<'_, Core>,
        first_l2_block_in_batch: &FirstL2BlockInBatch,
        validation_computational_gas_limit: u32,
        chain_id: L2ChainId,
    ) -> anyhow::Result<(SystemEnv, L1BatchEnv)> {
        anyhow::ensure!(
            first_l2_block_in_batch.l1_batch_number > L1BatchNumber(0),
            "Loading params for genesis L1 batch not supported"
        );
        // L1 batch timestamp is set to the timestamp of its first L2 block.
        let l1_batch_timestamp = first_l2_block_in_batch.header.timestamp;

        let prev_l1_batch_number = first_l2_block_in_batch.l1_batch_number - 1;
        tracing::info!("Getting previous L1 batch hash for batch #{prev_l1_batch_number}");
        let (prev_l1_batch_hash, prev_l1_batch_timestamp) = self
            .wait_for_l1_batch_params(storage, prev_l1_batch_number)
            .await
            .context("failed getting hash for previous L1 batch")?;
        tracing::info!("Got state root hash for previous L1 batch #{prev_l1_batch_number}: {prev_l1_batch_hash:?}");

        anyhow::ensure!(
            prev_l1_batch_timestamp < l1_batch_timestamp,
            "Invalid params for L1 batch #{}: Timestamp of previous L1 batch ({prev_l1_batch_timestamp}) >= \
             provisional L1 batch timestamp ({l1_batch_timestamp}), \
             meaning that L1 batch will be rejected by the bootloader",
            first_l2_block_in_batch.l1_batch_number
        );

        let prev_l2_block_number = first_l2_block_in_batch.header.number - 1;
        tracing::info!("Getting previous L2 block hash for L2 block #{prev_l2_block_number}");

        let prev_l2_block_hash = self.snapshot.as_ref().and_then(|snapshot| {
            (snapshot.l2_block_number == prev_l2_block_number).then_some(snapshot.l2_block_hash)
        });
        let prev_l2_block_hash = match prev_l2_block_hash {
            Some(hash) => hash,
            None => storage
                .blocks_web3_dal()
                .get_l2_block_hash(prev_l2_block_number)
                .await
                .map_err(DalError::generalize)?
                .context("previous L2 block disappeared from storage")?,
        };
        tracing::info!(
            "Got hash for previous L2 block #{prev_l2_block_number}: {prev_l2_block_hash:?}"
        );

        let contract_hashes = first_l2_block_in_batch.header.base_system_contracts_hashes;
        let base_system_contracts = storage
            .factory_deps_dal()
            .get_base_system_contracts(contract_hashes.bootloader, contract_hashes.default_aa)
            .await
            .context("failed getting base system contracts")?;

        Ok(l1_batch_params(
            first_l2_block_in_batch.l1_batch_number,
            first_l2_block_in_batch.header.fee_account_address,
            l1_batch_timestamp,
            prev_l1_batch_hash,
            first_l2_block_in_batch.header.batch_fee_input,
            first_l2_block_in_batch.header.number,
            prev_l2_block_hash,
            base_system_contracts,
            validation_computational_gas_limit,
            first_l2_block_in_batch
                .header
                .protocol_version
                .context("`protocol_version` must be set for L2 block")?,
            first_l2_block_in_batch.header.virtual_blocks,
            chain_id,
            PubdataParams {
                l2_da_validator_address: first_l2_block_in_batch
                    .header
                    .pubdata_params
                    .l2_da_validator_address,
                pubdata_type: first_l2_block_in_batch.header.pubdata_params.pubdata_type,
            },
        ))
    }
}
