use std::time::SystemTime;

use anyhow::Context;
use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_multivm::{
    interface::{L1BatchEnv, L2BlockEnv, OneshotEnv, StoredL2BlockEnv, SystemEnv, TxExecutionMode},
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};
use zksync_types::{
    api,
    block::{unpack_block_info, L2BlockHasher},
    fee_model::BatchFeeInput,
    get_deployer_key, h256_to_u256,
    settlement::SettlementLayer,
    AccountTreeId, L1BatchNumber, L2BlockNumber, ProtocolVersionId, StorageKey, H256,
    SYSTEM_CONTEXT_ADDRESS, SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION, ZKPORTER_IS_AVAILABLE,
};

use super::{env::OneshotEnvParameters, ContractsKind};

/// Block information necessary to execute a transaction / call. Unlike [`ResolvedBlockInfo`], this information is *partially* resolved,
/// which is beneficial for some data workflows.
#[derive(Debug, Clone, Copy)]
pub struct BlockInfo {
    resolved_block_number: L2BlockNumber,
    l1_batch_timestamp_s: Option<u64>,
    settlement_layer: SettlementLayer,
}

impl BlockInfo {
    /// Fetches information for a pending block.
    pub async fn pending(
        connection: &mut Connection<'_, Core>,
        settlement_layer: SettlementLayer,
    ) -> anyhow::Result<Self> {
        let resolved_block_number = connection
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Pending))
            .await
            .map_err(DalError::generalize)?
            .context("pending block should always be present in Postgres")?;
        Ok(Self {
            resolved_block_number,
            l1_batch_timestamp_s: None,
            settlement_layer,
        })
    }

    /// Fetches information for an existing block. Will error if the block is not present in Postgres.
    pub async fn for_existing_block(
        connection: &mut Connection<'_, Core>,
        number: L2BlockNumber,
    ) -> anyhow::Result<Self> {
        let l1_batch = connection
            .storage_web3_dal()
            .resolve_l1_batch_number_of_l2_block(number)
            .await
            .with_context(|| format!("failed resolving L1 batch number of L2 block #{number}"))?;
        let l1_batch_timestamp = connection
            .blocks_web3_dal()
            .get_expected_l1_batch_timestamp(&l1_batch)
            .await
            .map_err(DalError::generalize)?
            .context("missing timestamp for non-pending block")?;
        let settlement_layer = connection
            .blocks_web3_dal()
            .get_expected_settlement_layer(&l1_batch)
            // TODO handle unwrap
            .await?
            .unwrap();
        Ok(Self {
            resolved_block_number: number,
            l1_batch_timestamp_s: Some(l1_batch_timestamp),
            settlement_layer,
        })
    }

    /// Returns L2 block number.
    pub fn block_number(&self) -> L2BlockNumber {
        self.resolved_block_number
    }

    /// Returns L1 batch timestamp in seconds, or `None` for a pending block.
    pub fn l1_batch_timestamp(&self) -> Option<u64> {
        self.l1_batch_timestamp_s
    }

    fn is_pending_l2_block(&self) -> bool {
        self.l1_batch_timestamp_s.is_none()
    }

    /// Loads historical fee input for this block. If the block is not present in the DB, returns an error.
    pub async fn historical_fee_input(
        &self,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<BatchFeeInput> {
        let header = connection
            .blocks_dal()
            .get_l2_block_header(self.resolved_block_number)
            .await?
            .context("resolved L2 block is not in storage")?;
        Ok(header.batch_fee_input)
    }

    /// Resolves this information into [`ResolvedBlockInfo`].
    pub async fn resolve(
        &self,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<ResolvedBlockInfo> {
        let (state_l2_block_number, vm_l1_batch_number, l1_batch_timestamp);

        let l2_block_header = if let Some(l1_batch_timestamp_s) = self.l1_batch_timestamp_s {
            vm_l1_batch_number = connection
                .storage_web3_dal()
                .resolve_l1_batch_number_of_l2_block(self.resolved_block_number)
                .await
                .context("failed resolving L1 batch for L2 block")?
                .expected_l1_batch();
            l1_batch_timestamp = l1_batch_timestamp_s;
            state_l2_block_number = self.resolved_block_number;

            connection
                .blocks_dal()
                .get_l2_block_header(self.resolved_block_number)
                .await?
                .context("resolved L2 block disappeared from storage")?
        } else {
            vm_l1_batch_number = connection
                .blocks_dal()
                .get_sealed_l1_batch_number()
                .await?
                .context("no L1 batches in storage")?;
            let sealed_l2_block_header = connection
                .blocks_dal()
                .get_last_sealed_l2_block_header()
                .await?
                .context("no L2 blocks in storage")?;

            state_l2_block_number = sealed_l2_block_header.number;
            // Timestamp of the next L1 batch must be greater than the timestamp of the last L2 block.
            let current_timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .context("incorrect system time")?
                .as_secs();
            l1_batch_timestamp = current_timestamp.max(sealed_l2_block_header.timestamp + 1);
            sealed_l2_block_header
        };

        // Blocks without version specified are considered to be of `Version9`.
        // TODO: remove `unwrap_or` when protocol version ID will be assigned for each block.
        let protocol_version = l2_block_header
            .protocol_version
            .unwrap_or(ProtocolVersionId::last_potentially_undefined());

        let use_evm_emulator =
            Self::is_evm_emulation_enabled(connection, state_l2_block_number).await?;
        Ok(ResolvedBlockInfo {
            state_l2_block_number,
            state_l2_block_hash: l2_block_header.hash,
            vm_l1_batch_number,
            l1_batch_timestamp,
            protocol_version,
            use_evm_emulator,
            is_pending: self.is_pending_l2_block(),
            settlement_layer: self.settlement_layer,
        })
    }

    // Whether the EVM emulation is enabled is determined by a value inside `ContractDeployer`.
    async fn is_evm_emulation_enabled(
        connection: &mut Connection<'_, Core>,
        at_block: L2BlockNumber,
    ) -> anyhow::Result<bool> {
        let allowed_contract_types_hashed_key =
            get_deployer_key(H256::from_low_u64_be(1)).hashed_key();
        let storage_values = connection
            .storage_logs_dal()
            .get_storage_values(&[allowed_contract_types_hashed_key], at_block)
            .await?;
        let allowed_contract_types = storage_values
            .get(&allowed_contract_types_hashed_key)
            .copied()
            .flatten()
            .unwrap_or_default();
        Ok(match allowed_contract_types {
            val if val.is_zero() => false,
            val if val == H256::from_low_u64_be(1) => true,
            _ => {
                tracing::warn!(?allowed_contract_types, %at_block, "Unknown allowed contract types in ContractDeployer storage");
                false
            }
        })
    }
}

/// Resolved [`BlockInfo`] containing additional data from VM state.
#[derive(Debug, Clone)]
pub struct ResolvedBlockInfo {
    state_l2_block_number: L2BlockNumber,
    state_l2_block_hash: H256,
    vm_l1_batch_number: L1BatchNumber,
    l1_batch_timestamp: u64,
    protocol_version: ProtocolVersionId,
    use_evm_emulator: bool,
    is_pending: bool,
    settlement_layer: SettlementLayer,
}

impl ResolvedBlockInfo {
    /// L2 block number (as stored in Postgres). This number may differ from `block.number` provided to the VM.
    pub fn state_l2_block_number(&self) -> L2BlockNumber {
        self.state_l2_block_number
    }

    pub fn protocol_version(&self) -> ProtocolVersionId {
        self.protocol_version
    }

    pub fn use_evm_emulator(&self) -> bool {
        self.use_evm_emulator
    }
}

impl<C: ContractsKind> OneshotEnvParameters<C> {
    pub(super) async fn to_env_inner(
        &self,
        connection: &mut Connection<'_, Core>,
        execution_mode: TxExecutionMode,
        resolved_block_info: &ResolvedBlockInfo,
        fee_input: BatchFeeInput,
        enforced_base_fee: Option<u64>,
    ) -> anyhow::Result<OneshotEnv> {
        let (next_block, current_block) = load_l2_block_info(
            connection,
            resolved_block_info.is_pending,
            resolved_block_info,
        )
        .await?;

        let (system, l1_batch) = self
            .prepare_env(
                execution_mode,
                resolved_block_info,
                next_block,
                fee_input,
                enforced_base_fee,
            )
            .await?;

        Ok(OneshotEnv {
            system,
            l1_batch,
            current_block,
        })
    }

    async fn prepare_env(
        &self,
        execution_mode: TxExecutionMode,
        resolved_block_info: &ResolvedBlockInfo,
        next_block: L2BlockEnv,
        fee_input: BatchFeeInput,
        enforced_base_fee: Option<u64>,
    ) -> anyhow::Result<(SystemEnv, L1BatchEnv)> {
        let &Self {
            operator_account,
            validation_computational_gas_limit,
            chain_id,
            ..
        } = self;

        let system_env = SystemEnv {
            zk_porter_available: ZKPORTER_IS_AVAILABLE,
            version: resolved_block_info.protocol_version,
            base_system_smart_contracts: self
                .base_system_contracts
                .base_system_contracts(resolved_block_info)
                .await
                .context("failed getting base system contracts")?,
            bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
            execution_mode,
            default_validation_computational_gas_limit: validation_computational_gas_limit,
            chain_id,
        };
        let l1_batch_env = L1BatchEnv {
            previous_batch_hash: None,
            number: resolved_block_info.vm_l1_batch_number,
            timestamp: resolved_block_info.l1_batch_timestamp,
            fee_input,
            fee_account: *operator_account.address(),
            enforced_base_fee,
            first_l2_block: next_block,
            settlement_layer: resolved_block_info.settlement_layer,
        };
        Ok((system_env, l1_batch_env))
    }
}

async fn load_l2_block_info(
    connection: &mut Connection<'_, Core>,
    is_pending_block: bool,
    resolved_block_info: &ResolvedBlockInfo,
) -> anyhow::Result<(L2BlockEnv, Option<StoredL2BlockEnv>)> {
    let mut current_block = None;
    let next_block = read_stored_l2_block(connection, resolved_block_info.state_l2_block_number)
        .await
        .context("failed reading L2 block info")?;

    let next_block = if is_pending_block {
        L2BlockEnv {
            number: next_block.number + 1,
            timestamp: resolved_block_info.l1_batch_timestamp,
            prev_block_hash: resolved_block_info.state_l2_block_hash,
            // For simplicity, we assume each L2 block create one virtual block.
            // This may be wrong only during transition period.
            max_virtual_blocks_to_create: 1,
            interop_roots: vec![],
        }
    } else if next_block.number == 0 {
        // Special case:
        // - For environments, where genesis block was created before virtual block upgrade it doesn't matter what we put here.
        // - Otherwise, we need to put actual values here. We cannot create next L2 block with block_number=0 and `max_virtual_blocks_to_create=0`
        //   because of SystemContext requirements. But, due to intrinsics of SystemContext, block.number still will be resolved to 0.
        L2BlockEnv {
            number: 1,
            timestamp: 0,
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
            max_virtual_blocks_to_create: 1,
            interop_roots: vec![],
        }
    } else {
        // We need to reset L2 block info in storage to process transaction in the current block context.
        // Actual resetting will be done after `storage_view` is created.
        let prev_block_number = resolved_block_info.state_l2_block_number - 1;
        let prev_l2_block = read_stored_l2_block(connection, prev_block_number)
            .await
            .context("failed reading previous L2 block info")?;

        let mut prev_block_hash = connection
            .blocks_web3_dal()
            .get_l2_block_hash(prev_block_number)
            .await
            .map_err(DalError::generalize)?;
        if prev_block_hash.is_none() {
            // We might need to load the previous block hash from the snapshot recovery metadata
            let snapshot_recovery = connection
                .snapshot_recovery_dal()
                .get_applied_snapshot_status()
                .await
                .map_err(DalError::generalize)?;
            prev_block_hash = snapshot_recovery.and_then(|recovery| {
                (recovery.l2_block_number == prev_block_number).then_some(recovery.l2_block_hash)
            });
        }

        current_block = Some(prev_l2_block);
        L2BlockEnv {
            number: next_block.number,
            timestamp: next_block.timestamp,
            prev_block_hash: prev_block_hash.with_context(|| {
                format!("missing hash for previous L2 block #{prev_block_number}")
            })?,
            max_virtual_blocks_to_create: 1,
            interop_roots: vec![],
        }
    };

    Ok((next_block, current_block))
}

async fn read_stored_l2_block(
    connection: &mut Connection<'_, Core>,
    l2_block_number: L2BlockNumber,
) -> anyhow::Result<StoredL2BlockEnv> {
    let l2_block_info_key = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    );
    let l2_block_info = connection
        .storage_web3_dal()
        .get_historical_value_unchecked(l2_block_info_key.hashed_key(), l2_block_number)
        .await?;
    let (l2_block_number_from_state, timestamp) = unpack_block_info(h256_to_u256(l2_block_info));

    let l2_block_txs_rolling_hash_key = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
    );
    let txs_rolling_hash = connection
        .storage_web3_dal()
        .get_historical_value_unchecked(l2_block_txs_rolling_hash_key.hashed_key(), l2_block_number)
        .await?;

    Ok(StoredL2BlockEnv {
        number: l2_block_number_from_state as u32,
        timestamp,
        txs_rolling_hash,
    })
}
