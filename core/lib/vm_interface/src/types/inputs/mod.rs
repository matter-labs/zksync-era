use zksync_types::{
    l2::L2Tx, ExecuteTransactionCommon, Nonce, PackedEthSignature, Transaction, U256,
};

pub use self::{
    execution_mode::VmExecutionMode,
    l1_batch_env::L1BatchEnv,
    l2_block::{L2BlockEnv, StoredL2BlockEnv},
    system_env::{SystemEnv, TxExecutionMode},
};

mod execution_mode;
mod l1_batch_env;
mod l2_block;
mod system_env;

/// Full environment for oneshot transaction / call execution.
#[derive(Debug)]
pub struct OneshotEnv {
    /// System environment.
    pub system: SystemEnv,
    /// Part of the environment specific to an L1 batch.
    pub l1_batch: L1BatchEnv,
    /// Part of the environment representing the current L2 block. Can be used to override storage slots
    /// in the system context contract, which are set from `L1BatchEnv.first_l2_block` by default.
    pub current_block: Option<StoredL2BlockEnv>,
}

/// Executor-independent arguments necessary to for oneshot transaction execution.
///
/// # Developer guidelines
///
/// Please don't add fields that duplicate `SystemEnv` or `L1BatchEnv` information, since both of these
/// are also provided to an executor.
#[derive(Debug)]
pub struct TxExecutionArgs {
    /// Transaction / call itself.
    pub transaction: Transaction,
    /// Nonce override for the initiator account.
    pub enforced_nonce: Option<Nonce>,
    /// Balance added to the initiator account.
    pub added_balance: U256,
    /// If `true`, then the batch's L1 / pubdata gas price will be adjusted so that the transaction's gas per pubdata limit is <=
    /// to the one in the block. This is often helpful in case we want the transaction validation to work regardless of the
    /// current L1 prices for gas or pubdata.
    pub adjust_pubdata_price: bool,
}

impl TxExecutionArgs {
    pub fn for_validation(tx: L2Tx) -> Self {
        Self {
            enforced_nonce: Some(tx.nonce()),
            added_balance: U256::zero(),
            adjust_pubdata_price: true,
            transaction: tx.into(),
        }
    }

    pub fn for_eth_call(mut call: L2Tx) -> Self {
        if call.common_data.signature.is_empty() {
            call.common_data.signature = PackedEthSignature::default().serialize_packed().into();
        }

        Self {
            enforced_nonce: None,
            added_balance: U256::zero(),
            adjust_pubdata_price: false,
            transaction: call.into(),
        }
    }

    pub fn for_gas_estimate(transaction: Transaction) -> Self {
        // For L2 transactions we need to explicitly put enough balance into the account of the users
        // while for L1->L2 transactions the `to_mint` field plays this role
        let added_balance = match &transaction.common_data {
            ExecuteTransactionCommon::L2(data) => data.fee.gas_limit * data.fee.max_fee_per_gas,
            ExecuteTransactionCommon::L1(_) => U256::zero(),
            ExecuteTransactionCommon::ProtocolUpgrade(_) => U256::zero(),
        };

        Self {
            enforced_nonce: transaction.nonce(),
            added_balance,
            adjust_pubdata_price: true,
            transaction,
        }
    }
}

/// Inputs and outputs for all tracers supported for oneshot transaction / call execution.
#[derive(Debug, Default)]
pub struct OneshotTracingParams {
    /// Whether to trace contract calls.
    pub trace_calls: bool,
}
