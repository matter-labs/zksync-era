use std::{cell::RefCell, rc::Rc};

use multivm::{
    interface::{
        L2BlockEnv, TxExecutionMode, VmExecutionMode, VmExecutionResultAndLogs, VmInterface,
    },
    utils::get_max_gas_per_pubdata_byte,
    vm_latest::{constants::BATCH_COMPUTATIONAL_GAS_LIMIT, HistoryEnabled, TracerDispatcher, Vm},
};
use once_cell::sync::Lazy;
use zksync_contracts::{deployer_contract, BaseSystemContracts};
use zksync_state::{InMemoryStorage, StorageView};
use zksync_types::{
    block::L2BlockHasher,
    ethabi::{encode, Token},
    fee::Fee,
    fee_model::BatchFeeInput,
    helpers::unix_timestamp_ms,
    l2::L2Tx,
    utils::storage_key_for_eth_balance,
    Address, K256PrivateKey, L1BatchNumber, L2BlockNumber, L2ChainId, Nonce, ProtocolVersionId,
    Transaction, CONTRACT_DEPLOYER_ADDRESS, H256, U256,
};
use zksync_utils::bytecode::hash_bytecode;

mod instruction_counter;

/// Bytecodes have consist of an odd number of 32 byte words
/// This function "fixes" bytecodes of wrong length by cutting off their end.
pub fn cut_to_allowed_bytecode_size(bytes: &[u8]) -> Option<&[u8]> {
    let mut words = bytes.len() / 32;
    if words == 0 {
        return None;
    }

    if words & 1 == 0 {
        words -= 1;
    }
    Some(&bytes[..32 * words])
}

static STORAGE: Lazy<InMemoryStorage> = Lazy::new(|| {
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);

    // Give `PRIVATE_KEY` some money
    let key = storage_key_for_eth_balance(&PRIVATE_KEY.address());
    storage.set_value(key, zksync_utils::u256_to_h256(U256([0, 0, 1, 0])));

    storage
});

static SYSTEM_CONTRACTS: Lazy<BaseSystemContracts> = Lazy::new(BaseSystemContracts::load_from_disk);

static CREATE_FUNCTION_SIGNATURE: Lazy<[u8; 4]> = Lazy::new(|| {
    deployer_contract()
        .function("create")
        .unwrap()
        .short_signature()
});

static PRIVATE_KEY: Lazy<K256PrivateKey> =
    Lazy::new(|| K256PrivateKey::from_bytes(H256([42; 32])).expect("invalid key bytes"));

pub struct BenchmarkingVm(Vm<StorageView<&'static InMemoryStorage>, HistoryEnabled>);

impl BenchmarkingVm {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let timestamp = unix_timestamp_ms();

        Self(Vm::new(
            multivm::interface::L1BatchEnv {
                previous_batch_hash: None,
                number: L1BatchNumber(1),
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
            },
            multivm::interface::SystemEnv {
                zk_porter_available: false,
                version: ProtocolVersionId::latest(),
                base_system_smart_contracts: SYSTEM_CONTRACTS.clone(),
                bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
                execution_mode: TxExecutionMode::VerifyExecute,
                default_validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
                chain_id: L2ChainId::from(270),
            },
            Rc::new(RefCell::new(StorageView::new(&*STORAGE))),
        ))
    }

    pub fn run_transaction(&mut self, tx: &Transaction) -> VmExecutionResultAndLogs {
        self.0.push_transaction(tx.clone());
        self.0.execute(VmExecutionMode::OneTx)
    }

    pub fn instruction_count(&mut self, tx: &Transaction) -> usize {
        self.0.push_transaction(tx.clone());

        let count = Rc::new(RefCell::new(0));

        self.0.inspect(
            TracerDispatcher::new(vec![Box::new(
                instruction_counter::InstructionCounter::new(count.clone()),
            )]),
            VmExecutionMode::OneTx,
        );

        count.take()
    }
}

pub fn get_deploy_tx(code: &[u8]) -> Transaction {
    let params = [
        Token::FixedBytes(vec![0u8; 32]),
        Token::FixedBytes(hash_bytecode(code).0.to_vec()),
        Token::Bytes([].to_vec()),
    ];
    let calldata = CREATE_FUNCTION_SIGNATURE
        .iter()
        .cloned()
        .chain(encode(&params))
        .collect();

    let mut signed = L2Tx::new_signed(
        CONTRACT_DEPLOYER_ADDRESS,
        calldata,
        Nonce(0),
        Fee {
            gas_limit: U256::from(30000000u32),
            max_fee_per_gas: U256::from(250_000_000),
            max_priority_fee_per_gas: U256::from(0),
            gas_per_pubdata_limit: U256::from(get_max_gas_per_pubdata_byte(
                ProtocolVersionId::latest().into(),
            )),
        },
        U256::zero(),
        L2ChainId::from(270),
        &PRIVATE_KEY,
        vec![code.to_vec()], // maybe not needed?
        Default::default(),
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());

    signed.into()
}

#[cfg(test)]
mod tests {
    use zksync_contracts::read_bytecode;

    use crate::*;

    #[test]
    fn can_deploy_contract() {
        let test_contract = read_bytecode(
            "etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json",
        );
        let mut vm = BenchmarkingVm::new();
        let res = vm.run_transaction(&get_deploy_tx(&test_contract));

        assert!(matches!(
            res.result,
            multivm::interface::ExecutionResult::Success { .. }
        ));
    }
}
