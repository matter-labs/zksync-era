use once_cell::sync::{Lazy, OnceCell};
use ouroboros::self_referencing;
use vm::vm_with_bootloader::DerivedBlockContext;
use vm::vm_with_bootloader::TxExecutionMode::VerifyExecute;
use vm::vm_with_bootloader::{push_transaction_to_bootloader_memory, BlockContextMode};
use vm::{HistoryEnabled, OracleTools, VmInstance};
use zk_evm::block_properties::BlockProperties;
use zksync_config::constants::ethereum::MAX_GAS_PER_PUBDATA_BYTE;
use zksync_contracts::deployer_contract;
use zksync_state::{InMemoryStorage, StorageView};
use zksync_types::ethabi::{encode, Token};
use zksync_types::l2::L2Tx;
use zksync_types::utils::storage_key_for_eth_balance;
use zksync_types::{fee::Fee, Nonce, Transaction, H256, U256};
use zksync_types::{L2ChainId, PackedEthSignature, CONTRACT_DEPLOYER_ADDRESS};
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
static CREATE_FUNCTION_SIGNATURE: Lazy<[u8; 4]> = Lazy::new(|| {
    deployer_contract()
        .function("create")
        .unwrap()
        .short_signature()
});
const PRIVATE_KEY: H256 = H256([42; 32]);
static BLOCK_PROPERTIES: OnceCell<BlockProperties> = OnceCell::new();

pub struct BenchmarkingVm<'a>(BenchmarkingVmInner<'a>);

#[self_referencing]
struct BenchmarkingVmInner<'a> {
    storage_view: StorageView<&'a InMemoryStorage>,
    #[borrows(mut storage_view)]
    oracle_tools: OracleTools<'this, false, HistoryEnabled>,
    #[borrows(mut oracle_tools)]
    #[not_covariant]
    vm: Box<VmInstance<'this, HistoryEnabled>>,
}

impl BenchmarkingVm<'_> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let (block_context, block_properties) = vm::utils::create_test_block_params();
        let block_context = block_context.into();

        let block_properties = BLOCK_PROPERTIES.get_or_init(|| block_properties);

        Self(
            BenchmarkingVmInnerBuilder {
                storage_view: StorageView::new(&*STORAGE),
                oracle_tools_builder: |storage_view| {
                    vm::OracleTools::new(storage_view, HistoryEnabled)
                },
                vm_builder: |oracle_tools| {
                    vm::vm_with_bootloader::init_vm(
                        oracle_tools,
                        BlockContextMode::NewBlock(block_context, Default::default()),
                        block_properties,
                        VerifyExecute,
                        &vm::utils::BASE_SYSTEM_CONTRACTS,
                    )
                },
            }
            .build(),
        )
    }

    pub fn run_transaction(
        &mut self,
        tx: &Transaction,
    ) -> Result<vm::vm::VmTxExecutionResult, vm::TxRevertReason> {
        self.0.with_vm_mut(|vm| {
            push_transaction_to_bootloader_memory(vm, tx, VerifyExecute, None);
            vm.execute_next_tx(u32::MAX, false)
        })
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

    let (block_context, _) = vm::utils::create_test_block_params();
    let block_context: DerivedBlockContext = block_context.into();

    let mut signed = L2Tx::new_signed(
        CONTRACT_DEPLOYER_ADDRESS,
        calldata,
        Nonce(0),
        Fee {
            gas_limit: U256::from(10000000u32),
            max_fee_per_gas: U256::from(block_context.base_fee),
            max_priority_fee_per_gas: U256::from(0),
            gas_per_pubdata_limit: U256::from(MAX_GAS_PER_PUBDATA_BYTE),
        },
        U256::zero(),
        L2ChainId(270),
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
    use zksync_contracts::read_bytecode;
    use zksync_types::tx::tx_execution_info::TxExecutionStatus::Success;

    use crate::*;

    #[test]
    fn can_deploy_contract() {
        let test_contract = read_bytecode(
            "etc/contracts-test-data/artifacts-zk/contracts/counter/counter.sol/Counter.json",
        );
        let mut vm = BenchmarkingVm::new();
        let res = vm.run_transaction(&get_deploy_tx(&test_contract));

        match res {
            Ok(x) => assert_eq!(x.status, Success),
            Err(_) => panic!("should succeed"),
        }
    }
}
