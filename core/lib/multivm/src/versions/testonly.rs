use zksync_contracts::BaseSystemContracts;
use zksync_test_account::Account;
use zksync_types::{
    block::L2BlockHasher, fee_model::BatchFeeInput, helpers::unix_timestamp_ms,
    utils::storage_key_for_eth_balance, Address, L1BatchNumber, L2BlockNumber, L2ChainId,
    ProtocolVersionId, U256,
};
use zksync_utils::u256_to_h256;

use crate::{
    interface::{storage::InMemoryStorage, L1BatchEnv, L2BlockEnv, SystemEnv, TxExecutionMode},
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};

pub(super) fn default_system_env() -> SystemEnv {
    SystemEnv {
        zk_porter_available: false,
        version: ProtocolVersionId::latest(),
        base_system_smart_contracts: BaseSystemContracts::playground(),
        bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        execution_mode: TxExecutionMode::VerifyExecute,
        default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        chain_id: L2ChainId::from(270),
    }
}

pub(super) fn default_l1_batch(number: L1BatchNumber) -> L1BatchEnv {
    let timestamp = unix_timestamp_ms();
    L1BatchEnv {
        previous_batch_hash: None,
        number,
        timestamp,
        fee_input: BatchFeeInput::l1_pegged(
            50_000_000_000, // 50 gwei
            250_000_000,    // 0.25 gwei
        ),
        fee_account: Address::random(),
        enforced_base_fee: None,
        first_l2_block: L2BlockEnv {
            number: 1,
            timestamp,
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
            max_virtual_blocks_to_create: 100,
        },
    }
}

pub(super) fn make_account_rich(storage: &mut InMemoryStorage, account: &Account) {
    let key = storage_key_for_eth_balance(&account.address);
    storage.set_value(key, u256_to_h256(U256::from(10_u64.pow(19))));
}
