//! Shadow VM tests. Since there are no real VM implementations in the `vm_interface` crate where `ShadowVm` is defined,
//! these tests are placed here.

use assert_matches::assert_matches;
use zksync_test_contracts::{Account, LoadnextContractExecutionParams, TestContract, TxType};
use zksync_types::{
    block::L2BlockHasher, fee::Fee, AccountTreeId, Address, Execute, L1BatchNumber, L2BlockNumber,
    ProtocolVersionId, StorageKey, H256, U256,
};

use crate::{
    interface::{
        storage::{InMemoryStorage, ReadStorage, StorageView},
        utils::{ShadowVm, VmDump},
        ExecutionResult, L1BatchEnv, L2BlockEnv, VmFactory, VmInterface, VmInterfaceExt,
    },
    utils::get_max_gas_per_pubdata_byte,
    versions::testonly::{
        default_l1_batch, default_system_env, make_address_rich, ContractToDeploy,
    },
    vm_fast,
    vm_fast::FastValidationTracer,
    vm_latest,
    vm_latest::HistoryEnabled,
};

mod tests;

type ReferenceVm<S = InMemoryStorage> = vm_latest::Vm<StorageView<S>, HistoryEnabled>;
type ShadowedFastVm<S = InMemoryStorage, Tr = ()> =
    crate::vm_instance::ShadowedFastVm<S, Tr, FastValidationTracer>;

fn hash_block(block_env: &L2BlockEnv, tx_hashes: &[H256]) -> H256 {
    let mut hasher = L2BlockHasher::new(
        L2BlockNumber(block_env.number),
        block_env.timestamp,
        block_env.prev_block_hash,
    );
    for &tx_hash in tx_hashes {
        hasher.push_tx_hash(tx_hash);
    }
    hasher.finalize(ProtocolVersionId::latest())
}

fn tx_fee(gas_limit: u32) -> Fee {
    Fee {
        gas_limit: U256::from(gas_limit),
        max_fee_per_gas: U256::from(250_000_000),
        max_priority_fee_per_gas: U256::from(0),
        gas_per_pubdata_limit: U256::from(get_max_gas_per_pubdata_byte(
            ProtocolVersionId::latest().into(),
        )),
    }
}

#[derive(Debug)]
struct Harness {
    alice: Account,
    bob: Account,
    storage_contract: ContractToDeploy,
    storage_contract_abi: &'static ethabi::Contract,
    current_block: L2BlockEnv,
}

impl Harness {
    const STORAGE_CONTRACT_ADDRESS: Address = Address::repeat_byte(23);

    fn new(l1_batch_env: &L1BatchEnv) -> Self {
        Self {
            alice: Account::from_seed(0),
            bob: Account::from_seed(1),
            storage_contract: ContractToDeploy::new(
                TestContract::storage_test().bytecode.to_vec(),
                Self::STORAGE_CONTRACT_ADDRESS,
            ),
            storage_contract_abi: &TestContract::storage_test().abi,
            current_block: l1_batch_env.first_l2_block,
        }
    }

    fn setup_storage(&self, storage: &mut InMemoryStorage) {
        make_address_rich(storage, self.alice.address);
        make_address_rich(storage, self.bob.address);

        self.storage_contract.insert(storage);
        let storage_contract_key = StorageKey::new(
            AccountTreeId::new(Self::STORAGE_CONTRACT_ADDRESS),
            H256::zero(),
        );
        storage.set_value_hashed_enum(
            storage_contract_key.hashed_key(),
            999,
            H256::from_low_u64_be(42),
        );
    }

    fn assert_dump(dump: &mut VmDump) {
        assert_eq!(dump.l1_batch_number(), L1BatchNumber(1));
        let tx_counts_per_block: Vec<_> =
            dump.l2_blocks.iter().map(|block| block.txs.len()).collect();
        assert_eq!(tx_counts_per_block, [1, 2, 2, 0]);

        let storage_contract_key = StorageKey::new(
            AccountTreeId::new(Self::STORAGE_CONTRACT_ADDRESS),
            H256::zero(),
        );
        let value = dump.storage.read_value(&storage_contract_key);
        assert_eq!(value, H256::from_low_u64_be(42));
        let enum_index = dump.storage.get_enumeration_index(&storage_contract_key);
        assert_eq!(enum_index, Some(999));
    }

    fn new_block(&mut self, vm: &mut impl VmInterface, tx_hashes: &[H256]) {
        self.current_block = L2BlockEnv {
            number: self.current_block.number + 1,
            timestamp: self.current_block.timestamp + 1,
            prev_block_hash: hash_block(&self.current_block, tx_hashes),
            max_virtual_blocks_to_create: self.current_block.max_virtual_blocks_to_create,
        };
        vm.start_new_l2_block(self.current_block);
    }

    fn execute_on_vm(&mut self, vm: &mut impl VmInterface) {
        let transfer_exec = Execute {
            contract_address: Some(self.bob.address()),
            calldata: vec![],
            value: 1_000_000_000.into(),
            factory_deps: vec![],
        };
        let transfer_to_bob = self
            .alice
            .get_l2_tx_for_execute(transfer_exec.clone(), None);
        let (compression_result, exec_result) =
            vm.execute_transaction_with_bytecode_compression(transfer_to_bob.clone(), true);
        compression_result.unwrap();
        assert!(!exec_result.result.is_failed(), "{:#?}", exec_result);

        self.new_block(vm, &[transfer_to_bob.hash()]);

        let out_of_gas_transfer = self.bob.get_l2_tx_for_execute(
            transfer_exec.clone(),
            Some(tx_fee(200_000)), // high enough to pass validation
        );
        let (compression_result, exec_result) =
            vm.execute_transaction_with_bytecode_compression(out_of_gas_transfer.clone(), true);
        compression_result.unwrap();
        assert_matches!(exec_result.result, ExecutionResult::Revert { .. });

        let write_fn = self.storage_contract_abi.function("simpleWrite").unwrap();
        let simple_write_tx = self.alice.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(Self::STORAGE_CONTRACT_ADDRESS),
                calldata: write_fn.encode_input(&[]).unwrap(),
                value: 0.into(),
                factory_deps: vec![],
            },
            None,
        );
        let (compression_result, exec_result) =
            vm.execute_transaction_with_bytecode_compression(simple_write_tx.clone(), true);
        compression_result.unwrap();
        assert!(!exec_result.result.is_failed(), "{:#?}", exec_result);

        let storage_contract_key = StorageKey::new(
            AccountTreeId::new(Self::STORAGE_CONTRACT_ADDRESS),
            H256::zero(),
        );
        let storage_logs = &exec_result.logs.storage_logs;
        assert!(storage_logs.iter().any(|log| {
            log.log.key == storage_contract_key && log.previous_value == H256::from_low_u64_be(42)
        }));

        self.new_block(vm, &[out_of_gas_transfer.hash(), simple_write_tx.hash()]);

        let deploy_tx = self.alice.get_deploy_tx(
            TestContract::load_test().bytecode,
            Some(&[ethabi::Token::Uint(100.into())]),
            TxType::L2,
        );
        let (compression_result, exec_result) =
            vm.execute_transaction_with_bytecode_compression(deploy_tx.tx.clone(), true);
        compression_result.unwrap();
        assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

        let load_test_tx = self.bob.get_loadnext_transaction(
            deploy_tx.address,
            LoadnextContractExecutionParams::default(),
            TxType::L2,
        );
        let (compression_result, exec_result) =
            vm.execute_transaction_with_bytecode_compression(load_test_tx.clone(), true);
        compression_result.unwrap();
        assert!(!exec_result.result.is_failed(), "{exec_result:#?}");

        self.new_block(vm, &[deploy_tx.tx.hash(), load_test_tx.hash()]);
    }
}

fn sanity_check_vm<Vm>() -> (Vm, Harness)
where
    Vm: VmFactory<StorageView<InMemoryStorage>>,
{
    let system_env = default_system_env();
    let l1_batch_env = default_l1_batch(L1BatchNumber(1));
    let mut storage = InMemoryStorage::with_system_contracts();
    let mut harness = Harness::new(&l1_batch_env);
    harness.setup_storage(&mut storage);

    let storage = StorageView::new(storage).to_rc_ptr();
    let mut vm = Vm::new(l1_batch_env, system_env, storage);
    harness.execute_on_vm(&mut vm);
    (vm, harness)
}

#[test]
fn sanity_check_harness() {
    sanity_check_vm::<ReferenceVm>();
}

#[test]
fn sanity_check_harness_on_new_vm() {
    sanity_check_vm::<vm_fast::Vm<_>>();
}

#[test]
fn sanity_check_shadow_vm() {
    let system_env = default_system_env();
    let l1_batch_env = default_l1_batch(L1BatchNumber(1));
    let mut storage = InMemoryStorage::with_system_contracts();
    let mut harness = Harness::new(&l1_batch_env);
    harness.setup_storage(&mut storage);

    // We need separate storage views since they are mutated by the VM execution
    let main_storage = StorageView::new(&storage).to_rc_ptr();
    let shadow_storage = StorageView::new(&storage).to_rc_ptr();
    let mut vm = ShadowVm::<_, ReferenceVm<_>, ReferenceVm<_>>::with_custom_shadow(
        l1_batch_env,
        system_env,
        main_storage,
        shadow_storage,
    );
    harness.execute_on_vm(&mut vm);
}

#[test]
fn shadow_vm_basics() {
    let (vm, harness) = sanity_check_vm::<ShadowedFastVm>();
    let mut dump = vm.dump_state();
    Harness::assert_dump(&mut dump);

    // Test standard playback functionality.
    let replayed_dump = dump.clone().play_back::<ShadowedFastVm<_>>().dump_state();
    pretty_assertions::assert_eq!(replayed_dump, dump);

    // Check that the VM executes identically when reading from the original storage and one restored from the dump.
    let mut storage = InMemoryStorage::with_system_contracts();
    harness.setup_storage(&mut storage);
    let storage = StorageView::new(storage).to_rc_ptr();

    let vm = dump
        .clone()
        .play_back_custom(|l1_batch_env, system_env, dump_storage| {
            ShadowVm::<_, ReferenceVm, ReferenceVm<_>>::with_custom_shadow(
                l1_batch_env,
                system_env,
                storage,
                dump_storage,
            )
        });
    let new_dump = vm.dump_state();
    pretty_assertions::assert_eq!(new_dump, dump);
}
