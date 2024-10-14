use once_cell::sync::Lazy;
use zksync_contracts::BaseSystemContracts;
use zksync_multivm::{
    interface::{L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode},
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};
use zksync_types::{
    block::L2BlockHasher, fee_model::BatchFeeInput, vm::FastVmMode, Address, L1BatchNumber,
    L2BlockNumber, L2ChainId, ProtocolVersionId, H256, ZKPORTER_IS_AVAILABLE,
};

static BASE_SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> =
    Lazy::new(BaseSystemContracts::load_from_disk);

pub(crate) const FAST_VM_MODES: [FastVmMode; 3] =
    [FastVmMode::Old, FastVmMode::New, FastVmMode::Shadow];

pub(crate) fn default_system_env(execution_mode: TxExecutionMode) -> SystemEnv {
    SystemEnv {
        zk_porter_available: ZKPORTER_IS_AVAILABLE,
        version: ProtocolVersionId::latest(),
        base_system_smart_contracts: BASE_SYSTEM_CONTRACTS.clone(),
        bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        execution_mode,
        default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        chain_id: L2ChainId::default(),
    }
}

pub(crate) fn default_l1_batch_env(number: u32) -> L1BatchEnv {
    L1BatchEnv {
        previous_batch_hash: Some(H256::zero()),
        number: L1BatchNumber(number),
        timestamp: number.into(),
        fee_account: Address::repeat_byte(0x22),
        enforced_base_fee: None,
        first_l2_block: L2BlockEnv {
            number,
            timestamp: number.into(),
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(number - 1)),
            max_virtual_blocks_to_create: 1,
        },
        fee_input: BatchFeeInput::sensible_l1_pegged_default(),
    }
}
