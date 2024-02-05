use std::{future::Future, pin::Pin};

use anyhow::Context;
use multivm::{
    interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode},
    vm_latest::constants::BLOCK_GAS_LIMIT,
};
use zksync_contracts::BaseSystemContracts;
use zksync_dal::StorageProcessor;
use zksync_types::{
    fee_model::BatchFeeInput, web3::futures::future::BoxFuture, Address, L1BatchNumber, L2ChainId,
    MiniblockNumber, ProtocolVersionId, H256, ZKPORTER_IS_AVAILABLE,
};

use crate::db::wait_for_prev_l1_batch_params;

/// The environment required to initialize the VM.
#[derive(Clone, Debug)]
pub struct VmEnv {
    pub l1_batch_env: L1BatchEnv,
    pub system_env: SystemEnv,
}

/// The builder for the `VmEnv`. It allows to load the required data from the database.
#[derive(Clone, Debug)]
pub struct VmEnvBuilder {
    miniblock_number: Option<MiniblockNumber>,
    l1_batch_number: L1BatchNumber,
    prev_batch_hash: Option<H256>,
    fee_account: Option<Address>,
    validation_computational_gas_limit: u32,
    chain_id: L2ChainId,
}

impl VmEnvBuilder {
    pub fn new(
        l1_batch_number: L1BatchNumber,
        validation_computational_gas_limit: u32,
        chain_id: L2ChainId,
    ) -> Self {
        Self {
            l1_batch_number,
            validation_computational_gas_limit,
            chain_id,
            miniblock_number: None,
            prev_batch_hash: None,
            fee_account: None,
        }
    }

    pub fn override_fee_account(mut self, fee_account: Address) -> Self {
        self.fee_account = Some(fee_account);
        self
    }

    /// Builds the `VmEnv` for the executed L1 batch. It verifies that the previous L1 batch is sealed and miniblock
    /// is included in the L1 batch.
    pub async fn build_for_executed_batch(
        mut self,
        connection: &mut StorageProcessor<'_>,
    ) -> anyhow::Result<VmEnv> {
        let batch_hash = connection
            .blocks_dal()
            .get_l1_batch_state_root_and_timestamp(self.l1_batch_number - 1)
            .await?
            .with_context(|| format!("Previous L1 batch not found {}", self.l1_batch_number - 1))?
            .0;

        let (first_miniblock_number_included_in_l1_batch, _) = connection
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(self.l1_batch_number)
            .await?
            .with_context(|| format!("L1 batch {} not found", self.l1_batch_number))?;

        self.miniblock_number = Some(first_miniblock_number_included_in_l1_batch);
        self.prev_batch_hash = Some(batch_hash);
        self.build(connection).await
    }

    /// Builds the `VmEnv` for the pending L1 batch.
    /// the previous L1 batch must be sealed.
    pub async fn build_for_pending_batch<F>(
        mut self,
        get_prev_batch_hash: F,
        connection: &mut StorageProcessor<'_>,
    ) -> anyhow::Result<VmEnv>
    where
        F: FnOnce(
            &mut StorageProcessor<'_>,
            L1BatchNumber,
        ) -> BoxFuture<'static, anyhow::Result<H256>>,
    {
        let batch_hash = get_prev_batch_hash(connection, self.l1_batch_number - 1).await?;

        let (_, last_miniblock_number_included_in_l1_batch) = connection
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(self.l1_batch_number)
            .await?
            .with_context(|| format!("L1 batch {} not found", self.l1_batch_number - 1))?;

        self.miniblock_number = Some(last_miniblock_number_included_in_l1_batch + 1);
        self.prev_batch_hash = Some(batch_hash);
        self.build(connection).await
    }

    async fn build(self, connection: &mut StorageProcessor<'_>) -> anyhow::Result<VmEnv> {
        let pending_miniblock_number = self
            .miniblock_number
            .context("Miniblock number not found")?;

        let pending_miniblock_header = connection
            .blocks_dal()
            .get_miniblock_header(pending_miniblock_number)
            .await?
            .with_context(|| format!("Miniblock {} not found", pending_miniblock_number))?;

        tracing::info!("Getting previous miniblock hash");
        let prev_miniblock_hash = connection
            .blocks_dal()
            .get_miniblock_header(pending_miniblock_number - 1)
            .await?
            .with_context(|| {
                format!(
                    "Previous Miniblock {} not found",
                    pending_miniblock_number - 1
                )
            })?
            .hash;
        let fee_account = if let Some(fee_account) = self.fee_account {
            fee_account
        } else {
            connection
                .blocks_dal()
                .get_fee_address_for_miniblock(pending_miniblock_number)
                .await?
                .context("Fee account not found")?
        };
        let base_system_contracts = connection
            .storage_dal()
            .get_base_system_contracts(
                pending_miniblock_header
                    .base_system_contracts_hashes
                    .bootloader,
                pending_miniblock_header
                    .base_system_contracts_hashes
                    .default_aa,
            )
            .await;

        let Some(previous_l1_batch_hash) = self.prev_batch_hash else {
            anyhow::bail!("Previous L1 batch hash not found")
        };
        tracing::info!("Previous l1_batch_hash: {}", previous_l1_batch_hash);
        let (system_env, l1_batch_env) = l1_batch_params(
            self.l1_batch_number,
            fee_account,
            pending_miniblock_header.timestamp,
            previous_l1_batch_hash,
            pending_miniblock_header.batch_fee_input,
            pending_miniblock_number,
            prev_miniblock_hash,
            base_system_contracts,
            self.validation_computational_gas_limit,
            pending_miniblock_header
                .protocol_version
                .expect("`protocol_version` must be set for pending miniblock"),
            pending_miniblock_header.virtual_blocks,
            self.chain_id,
        );
        Ok(VmEnv {
            l1_batch_env,
            system_env,
        })
    }
}

// TODO: Make it private and move this initialization to the `VmEnvBuilder`.
/// Returns the parameters required to initialize the VM for the next L1 batch.
#[allow(clippy::too_many_arguments)]
pub fn l1_batch_params(
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    l1_batch_timestamp: u64,
    previous_batch_hash: H256,
    fee_input: BatchFeeInput,
    first_miniblock_number: MiniblockNumber,
    prev_miniblock_hash: H256,
    base_system_contracts: BaseSystemContracts,
    validation_computational_gas_limit: u32,
    protocol_version: ProtocolVersionId,
    virtual_blocks: u32,
    chain_id: L2ChainId,
) -> (SystemEnv, L1BatchEnv) {
    (
        SystemEnv {
            zk_porter_available: ZKPORTER_IS_AVAILABLE,
            version: protocol_version,
            base_system_smart_contracts: base_system_contracts,
            gas_limit: BLOCK_GAS_LIMIT,
            execution_mode: TxExecutionMode::VerifyExecute,
            default_validation_computational_gas_limit: validation_computational_gas_limit,
            chain_id,
        },
        L1BatchEnv {
            previous_batch_hash: Some(previous_batch_hash),
            number: current_l1_batch_number,
            timestamp: l1_batch_timestamp,
            fee_input,
            fee_account,
            enforced_base_fee: None,
            first_l2_block: L2BlockEnv {
                number: first_miniblock_number.0,
                timestamp: l1_batch_timestamp,
                prev_block_hash: prev_miniblock_hash,
                max_virtual_blocks_to_create: virtual_blocks,
            },
        },
    )
}
