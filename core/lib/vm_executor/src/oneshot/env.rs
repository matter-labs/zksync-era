use std::sync::Arc;

use zksync_dal::{Connection, Core};
use zksync_multivm::interface::{OneshotEnv, TxExecutionMode};
use zksync_types::{fee_model::BatchFeeInput, l2::L2Tx, AccountTreeId, L2ChainId};

use super::{
    BaseSystemContractsProvider, CallOrExecute, ContractsKind, EstimateGas, ResolvedBlockInfo,
};

/// Oneshot environment parameters that are expected to be constant or rarely change during the program lifetime.
///
/// These parameters can be used to create [a full environment](OneshotEnv) for transaction / call execution.
///
/// Notably, these parameters include base system contracts (bootloader and default account abstraction) for all supported
/// VM versions.
#[derive(Debug)]
pub struct OneshotEnvParameters<C: ContractsKind> {
    pub(super) chain_id: L2ChainId,
    pub(super) base_system_contracts: Arc<dyn BaseSystemContractsProvider<C>>,
    pub(super) operator_account: AccountTreeId,
    pub(super) validation_computational_gas_limit: u32,
}

impl<C: ContractsKind> OneshotEnvParameters<C> {
    /// Creates env parameters.
    pub fn new(
        base_system_contracts: Arc<dyn BaseSystemContractsProvider<C>>,
        chain_id: L2ChainId,
        operator_account: AccountTreeId,
        validation_computational_gas_limit: u32,
    ) -> Self {
        Self {
            chain_id,
            base_system_contracts,
            operator_account,
            validation_computational_gas_limit,
        }
    }

    /// Returns gas limit for account validation of transactions.
    pub fn validation_computational_gas_limit(&self) -> u32 {
        self.validation_computational_gas_limit
    }
}

impl OneshotEnvParameters<EstimateGas> {
    /// Prepares environment for gas estimation.
    pub async fn to_env(
        &self,
        connection: &mut Connection<'_, Core>,
        resolved_block_info: &ResolvedBlockInfo,
        fee_input: BatchFeeInput,
        base_fee: u64,
    ) -> anyhow::Result<OneshotEnv> {
        self.to_env_inner(
            connection,
            TxExecutionMode::EstimateFee,
            resolved_block_info,
            fee_input,
            Some(base_fee),
        )
        .await
    }
}

impl OneshotEnvParameters<CallOrExecute> {
    /// Prepares environment for a call.
    pub async fn to_call_env(
        &self,
        connection: &mut Connection<'_, Core>,
        resolved_block_info: &ResolvedBlockInfo,
        fee_input: BatchFeeInput,
        enforced_base_fee: Option<u64>,
    ) -> anyhow::Result<OneshotEnv> {
        self.to_env_inner(
            connection,
            TxExecutionMode::EthCall,
            resolved_block_info,
            fee_input,
            enforced_base_fee,
        )
        .await
    }

    /// Prepares environment for executing a provided transaction.
    pub async fn to_execute_env(
        &self,
        connection: &mut Connection<'_, Core>,
        resolved_block_info: &ResolvedBlockInfo,
        fee_input: BatchFeeInput,
        tx: &L2Tx,
    ) -> anyhow::Result<OneshotEnv> {
        self.to_env_inner(
            connection,
            TxExecutionMode::VerifyExecute,
            resolved_block_info,
            fee_input,
            Some(tx.common_data.fee.max_fee_per_gas.as_u64()),
        )
        .await
    }
}
