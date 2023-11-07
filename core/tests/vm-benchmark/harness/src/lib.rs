use multivm::interface::{
    L2BlockEnv, TxExecutionMode, VmExecutionMode, VmExecutionResultAndLogs, VmInterface,
};
use multivm::vm_latest::{constants::BLOCK_GAS_LIMIT, HistoryEnabled, Vm};
use once_cell::sync::Lazy;
use std::{cell::RefCell, rc::Rc};
use zksync_contracts::{deployer_contract, BaseSystemContracts};
use zksync_state::{InMemoryStorage, StorageView};
use zksync_system_constants::ethereum::MAX_GAS_PER_PUBDATA_BYTE;
use zksync_types::{
    block::legacy_miniblock_hash,
    ethabi::{encode, Token},
    fee::Fee,
    helpers::unix_timestamp_ms,
    l2::L2Tx,
    utils::storage_key_for_eth_balance,
    Address, L1BatchNumber, L2ChainId, MiniblockNumber, Nonce, PackedEthSignature,
    ProtocolVersionId, Transaction, CONTRACT_DEPLOYER_ADDRESS, H256, U256,
};
use zksync_utils::bytecode::hash_bytecode;

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

    // give PRIVATE_KEY some money
    let my_addr = PackedEthSignature::address_from_private_key(&PRIVATE_KEY).unwrap();
    let key = storage_key_for_eth_balance(&my_addr);
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
const PRIVATE_KEY: H256 = H256([42; 32]);

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
                l1_gas_price: 50_000_000_000,   // 50 gwei
                fair_l2_gas_price: 250_000_000, // 0.25 gwei
                fee_account: Address::random(),
                enforced_base_fee: None,
                first_l2_block: L2BlockEnv {
                    number: 1,
                    timestamp,
                    prev_block_hash: legacy_miniblock_hash(MiniblockNumber(0)),
                    max_virtual_blocks_to_create: 100,
                },
            },
            multivm::interface::SystemEnv {
                zk_porter_available: false,
                version: ProtocolVersionId::latest(),
                base_system_smart_contracts: SYSTEM_CONTRACTS.clone(),
                gas_limit: BLOCK_GAS_LIMIT,
                execution_mode: TxExecutionMode::VerifyExecute,
                default_validation_computational_gas_limit: BLOCK_GAS_LIMIT,
                chain_id: L2ChainId::from(270),
            },
            Rc::new(RefCell::new(StorageView::new(&*STORAGE))),
        ))
    }

    pub fn run_transaction(&mut self, tx: &Transaction) -> VmExecutionResultAndLogs {
        self.0.push_transaction(tx.clone());
        self.0.execute(VmExecutionMode::OneTx)
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
            gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
        },
        U256::zero(),
        L2ChainId::from(270),
        &PRIVATE_KEY,
        Some(vec![code.to_vec()]), // maybe not needed?
        Default::default(),
    )
    .expect("should create a signed execute transaction");

    signed.set_input(H256::random().as_bytes().to_vec(), H256::random());

    signed.into()
}

#[cfg(test)]
mod tests {
    use crate::*;
    use zksync_contracts::read_bytecode;

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
