//! Shadow VM tests.

use assert_matches::assert_matches;
use zksync_contracts::{get_loadnext_contract, test_contracts::LoadnextContractExecutionParams};
use zksync_test_account::{Account, TxType};
use zksync_types::{
    block::L2BlockHasher, fee::Fee, Execute, L1BatchNumber, L2BlockNumber, ProtocolVersionId, H256,
    U256,
};
use zksync_utils::bytecode::hash_bytecode;

use super::*;
use crate::{
    dump::VmAction,
    interface::{storage::InMemoryStorage, ExecutionResult},
    utils::get_max_gas_per_pubdata_byte,
    versions::testonly::{default_l1_batch, default_system_env, make_account_rich},
    vm_latest,
    vm_latest::HistoryEnabled,
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
    current_block: L2BlockEnv,
}

impl Harness {
    fn new(l1_batch_env: &L1BatchEnv) -> Self {
        Self {
            alice: Account::random(),
            bob: Account::random(),
            current_block: l1_batch_env.first_l2_block,
        }
    }

    fn setup_storage(&self, storage: &mut InMemoryStorage) {
        make_account_rich(storage, &self.alice);
        make_account_rich(storage, &self.bob);
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

        self.new_block(vm, &[out_of_gas_transfer.hash()]);

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
    let (vm, harness) = sanity_check_vm::<ShadowVm<_, ReferenceVm>>();
    let dump = vm.dump_state();
    assert_eq!(dump.l1_batch_number(), L1BatchNumber(1));
    assert_matches!(
        dump.actions.as_slice(),
        [
            VmAction::Transaction(_),
            VmAction::Block(_),
            VmAction::Transaction(_),
            VmAction::Block(_),
            VmAction::Transaction(_),
            VmAction::Transaction(_),
            VmAction::Block(_),
        ]
    );
    assert!(!dump.storage.read_storage_keys.is_empty());
    assert!(!dump.storage.factory_deps.is_empty());

    dbg!(111);

    let mut storage = InMemoryStorage::with_system_contracts(hash_bytecode);
    harness.setup_storage(&mut storage);
    let storage = StorageView::new(storage).to_rc_ptr();

    let dump_storage = dump.storage.clone().into_storage();
    let dump_storage = StorageView::new(dump_storage).to_rc_ptr();
    // Check that the VM executes identically when reading from the original storage and one restored from the dump.
    let mut vm = ShadowVm::<_, ReferenceVm, ReferenceVm>::with_custom_shadow(
        dump.l1_batch_env.clone(),
        dump.system_env.clone(),
        storage,
        dump_storage,
    );

    for action in dump.actions.clone() {
        match dbg!(action) {
            VmAction::Transaction(tx) => {
                let (compression_result, _) =
                    vm.execute_transaction_with_bytecode_compression(*tx, true);
                compression_result.unwrap();
            }
            VmAction::Block(block) => {
                vm.start_new_l2_block(block);
            }
        }
    }
    vm.finish_batch();

    let new_dump = vm.dump_state();
    pretty_assertions::assert_eq!(new_dump, dump);
}
