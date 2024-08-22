//! Shadow VM tests.

use assert_matches::assert_matches;
use ethabi::Contract;
use zksync_contracts::{
    get_loadnext_contract, load_contract, read_bytecode,
    test_contracts::LoadnextContractExecutionParams,
};
use zksync_test_account::{Account, TxType};
use zksync_types::{
    block::L2BlockHasher, fee::Fee, AccountTreeId, Address, Execute, L1BatchNumber, L2BlockNumber,
    ProtocolVersionId, H256, U256,
};
use zksync_utils::bytecode::hash_bytecode;

use super::*;
use crate::{
    dump::VmAction,
    interface::{storage::InMemoryStorage, ExecutionResult},
    utils::get_max_gas_per_pubdata_byte,
    versions::testonly::{
        default_l1_batch, default_system_env, make_account_rich, ContractToDeploy,
    },
    vm_latest::{self, HistoryDisabled},
};

type ReferenceVm<S = InMemoryStorage> = vm_latest::Vm<StorageView<S>, HistoryEnabled>;

fn hash_block(block_env: L2BlockEnv, tx_hashes: &[H256]) -> H256 {
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
    storage_contract_abi: Contract,
    current_block: L2BlockEnv,
}

impl Harness {
    const STORAGE_CONTRACT_PATH: &'static str =
        "etc/contracts-test-data/artifacts-zk/contracts/storage/storage.sol/StorageTester.json";
    const STORAGE_CONTRACT_ADDRESS: Address = Address::repeat_byte(23);

    fn new(l1_batch_env: &L1BatchEnv) -> Self {
        Self {
            alice: Account::random(),
            bob: Account::random(),
            storage_contract: ContractToDeploy::new(
                read_bytecode(Self::STORAGE_CONTRACT_PATH),
                Self::STORAGE_CONTRACT_ADDRESS,
            ),
            storage_contract_abi: load_contract(Self::STORAGE_CONTRACT_PATH),
            current_block: l1_batch_env.first_l2_block,
        }
    }

    fn setup_storage(&self, storage: &mut InMemoryStorage) {
        make_account_rich(storage, &self.alice);
        make_account_rich(storage, &self.bob);

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

    fn assert_dump(dump: &VmDump) {
        assert_eq!(dump.l1_batch_number(), L1BatchNumber(1));
        assert_matches!(
            dump.actions.as_slice(),
            [
                VmAction::Transaction(_),
                VmAction::Block(_),
                VmAction::Transaction(_),
                VmAction::Transaction(_),
                VmAction::Block(_),
                VmAction::Transaction(_),
                VmAction::Transaction(_),
                VmAction::Block(_),
            ]
        );
        assert!(!dump.storage.read_storage_keys.is_empty());
        assert!(!dump.storage.factory_deps.is_empty());

        let storage_contract_key = StorageKey::new(
            AccountTreeId::new(Self::STORAGE_CONTRACT_ADDRESS),
            H256::zero(),
        )
        .hashed_key();
        let (value, enum_index) = dump.storage.read_storage_keys[&storage_contract_key];
        assert_eq!(value, H256::from_low_u64_be(42));
        assert_eq!(enum_index, 999);
    }

    fn new_block(&mut self, vm: &mut impl VmInterface, tx_hashes: &[H256]) {
        self.current_block = L2BlockEnv {
            number: self.current_block.number + 1,
            timestamp: self.current_block.timestamp + 1,
            prev_block_hash: hash_block(self.current_block, tx_hashes),
            max_virtual_blocks_to_create: self.current_block.max_virtual_blocks_to_create,
        };
        vm.start_new_l2_block(self.current_block);
    }

    fn execute_on_vm(&mut self, vm: &mut impl VmInterface) {
        let transfer_exec = Execute {
            contract_address: self.bob.address(),
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
                contract_address: Self::STORAGE_CONTRACT_ADDRESS,
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
            &get_loadnext_contract().bytecode,
            Some(&[ethabi::Token::Uint(100.into())]),
            TxType::L2,
        );
        let (compression_result, exec_result) =
            vm.execute_transaction_with_bytecode_compression(deploy_tx.tx.clone(), true);
        compression_result.unwrap();
        assert!(!exec_result.result.is_failed(), "{:#?}", exec_result);

        let load_test_tx = self.bob.get_loadnext_transaction(
            deploy_tx.address,
            LoadnextContractExecutionParams::default(),
            TxType::L2,
        );
        let (compression_result, exec_result) =
            vm.execute_transaction_with_bytecode_compression(load_test_tx.clone(), true);
        compression_result.unwrap();
        assert!(!exec_result.result.is_failed(), "{:#?}", exec_result);

        self.new_block(vm, &[deploy_tx.tx.hash(), load_test_tx.hash()]);
        vm.finish_batch();
    }
}

fn sanity_check_vm<Vm>() -> (Vm, Harness)
where
    Vm: VmInterface + VmFactory<StorageView<InMemoryStorage>>,
{
    let system_env = default_system_env();
    let l1_batch_env = default_l1_batch(L1BatchNumber(1));
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
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
    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    let mut harness = Harness::new(&l1_batch_env);
    harness.setup_storage(&mut storage);

    // We need separate storage views since they are mutated by the VM execution
    let main_storage = StorageView::new(&storage).to_rc_ptr();
    let shadow_storage = StorageView::new(&storage).to_rc_ptr();
    let mut vm = ShadowVm::<_, HistoryDisabled, ReferenceVm<_>>::with_custom_shadow(
        l1_batch_env,
        system_env,
        main_storage,
        shadow_storage,
    );
    harness.execute_on_vm(&mut vm);
}

#[test]
fn shadow_vm_basics() {
    let (mut vm, harness) = sanity_check_vm::<ShadowVm<_, HistoryDisabled>>();
    let dump = vm.dump_state();
    Harness::assert_dump(&dump);

    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    harness.setup_storage(&mut storage);
    let storage = StorageView::new(storage).to_rc_ptr();

    let dump_storage = dump.storage.clone().into_storage();
    let dump_storage = StorageView::new(dump_storage).to_rc_ptr();
    // Check that the VM executes identically when reading from the original storage and one restored from the dump.
    let mut vm = ShadowVm::<_, HistoryDisabled, ReferenceVm>::with_custom_shadow(
        dump.l1_batch_env.clone(),
        dump.system_env.clone(),
        storage,
        dump_storage,
    );

    for action in dump.actions.clone() {
        match action {
            VmAction::Transaction(tx) => {
                let (compression_result, _) =
                    vm.execute_transaction_with_bytecode_compression(*tx, true);
                compression_result.unwrap();
            }
            VmAction::Block(block) => {
                vm.start_new_l2_block(block);
            }
        }
        vm.dump_state(); // Check that a dump can be created at any point in batch execution
    }
    vm.finish_batch();

    let new_dump = vm.dump_state();
    pretty_assertions::assert_eq!(new_dump, dump);
}
