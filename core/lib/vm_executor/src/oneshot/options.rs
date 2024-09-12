use std::marker::PhantomData;

use anyhow::Context;
use zksync_dal::{Connection, Core};
use zksync_multivm::interface::{OneshotEnv, TxExecutionMode};
use zksync_types::{fee_model::BatchFeeInput, l2::L2Tx, AccountTreeId, L2ChainId};

use crate::oneshot::{contracts::MultiVMBaseSystemContracts, ResolvedBlockInfo};

/// Marker for [`OneshotExecutorOptions`] used for gas estimation.
#[derive(Debug)]
pub struct EstimateGas(());

/// Marker for [`OneshotExecutorOptions`] used for calls and/or transaction execution.
#[derive(Debug)]
pub struct CallOrExecute(());

/// Oneshot executor options that are expected to be constant or rarely change during the program lifetime.
/// These options can be used to create [a full environment](OneshotEnv) for transaction / call execution.
#[derive(Debug)]
pub struct OneshotExecutorOptions<T> {
    pub(super) chain_id: L2ChainId,
    pub(super) base_system_contracts: MultiVMBaseSystemContracts,
    pub(super) operator_account: AccountTreeId,
    pub(super) validation_computational_gas_limit: u32,
    _ty: PhantomData<T>,
}

impl<T> OneshotExecutorOptions<T> {
    /// Returns gas limit for account validation of transactions.
    pub fn validation_computational_gas_limit(&self) -> u32 {
        self.validation_computational_gas_limit
    }
}

impl OneshotExecutorOptions<EstimateGas> {
    /// Creates executor options for gas estimation.
    pub async fn for_gas_estimation(
        chain_id: L2ChainId,
        operator_account: AccountTreeId,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            chain_id,
            base_system_contracts: tokio::task::spawn_blocking(
                MultiVMBaseSystemContracts::load_estimate_gas_blocking,
            )
            .await
            .context("failed loading system contracts for gas estimation")?,
            operator_account,
            validation_computational_gas_limit: u32::MAX,
            _ty: PhantomData,
        })
    }

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

impl OneshotExecutorOptions<CallOrExecute> {
    /// Creates executor options for transaction / call execution.
    pub async fn for_execution(
        chain_id: L2ChainId,
        operator_account: AccountTreeId,
        validation_computational_gas_limit: u32,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            chain_id,
            base_system_contracts: tokio::task::spawn_blocking(
                MultiVMBaseSystemContracts::load_eth_call_blocking,
            )
            .await
            .context("failed loading system contracts for calls")?,
            operator_account,
            validation_computational_gas_limit,
            _ty: PhantomData,
        })
    }

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
