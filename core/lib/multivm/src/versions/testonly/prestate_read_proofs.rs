//! Re-executing a batch from the deduplicated logs' pre-states plus the declared "gap" keys at
//! their proven pre-state (what the Airbender read proofs deliver) reproduces it exactly; serving
//! the gap keys as empty does not.

use std::collections::{HashMap, HashSet};

use ethabi::Token;
use zksync_test_contracts::{Account, TestContract};
use zksync_types::{
    bytecode::BytecodeHash, fee::Fee, fee_model::BatchFeeInput, get_known_code_key, u256_to_h256,
    writes::StateDiffRecord, AccountTreeId, Address, Execute, L1BatchNumber, StorageKey,
    Transaction, H256, U256,
};

use super::{
    default_l1_batch, default_pubdata_builder, tester::VmTesterBuilder, ContractToDeploy, TestedVm,
};
use crate::interface::{
    storage::{InMemoryStorage, ReadStorage, StorageSnapshot, StorageView},
    CompressedBytecodeInfo, ExecutionResult, L1BatchEnv, SystemEnv, TxExecutionMode, VmFactory,
    VmInterface, VmInterfaceExt,
};

/// Everything a re-execution must reproduce.
#[derive(Debug, PartialEq)]
struct BatchOutcome {
    result: ExecutionResult,
    compressed_bytecodes: Vec<CompressedBytecodeInfo>,
    bootloader_memory: Vec<(usize, U256)>,
    state_diffs: Vec<StateDiffRecord>,
    pubdata: Vec<u8>,
}

/// `None` if a bytecode the host compressed stays unpublished, which ends the batch there; the
/// legacy VM does this when a rerun serves a known-code marker as empty.
fn run_one_tx(
    vm: &mut impl VmInterface,
    tx: Transaction,
) -> Option<(BatchOutcome, Vec<StorageKey>)> {
    let (compressed, result) = vm.execute_transaction_with_bytecode_compression(tx, true);
    let compressed_bytecodes = compressed.ok()?.into_owned();
    let batch = vm.finish_batch(default_pubdata_builder());
    let dedup_keys = batch
        .final_execution_state
        .deduplicated_storage_logs
        .iter()
        .map(|log| log.key)
        .collect();
    let outcome = BatchOutcome {
        result: result.result,
        compressed_bytecodes,
        bootloader_memory: batch.final_bootloader_memory.expect("no bootloader memory"),
        state_diffs: batch.state_diffs.expect("no state diffs"),
        pubdata: batch.pubdata_input.expect("no pubdata input"),
    };
    Some((outcome, dedup_keys))
}

/// A batch executed on real storage, plus what re-execution from a snapshot needs.
struct Recorded {
    outcome: BatchOutcome,
    /// Keys the deduplicated storage logs cover, i.e. what `merkle_paths` proves.
    dedup_keys: HashSet<StorageKey>,
    /// Keys the storage view recorded: the witness' `read_storage_key ∪ is_write_initial`.
    declared_keys: HashSet<StorageKey>,
    pre_batch: InMemoryStorage,
    factory_deps: HashMap<H256, Vec<u8>>,
    l1_batch_env: L1BatchEnv,
    system_env: SystemEnv,
}

impl Recorded {
    fn prestate(&self, key: &StorageKey) -> Option<(H256, u64)> {
        let mut storage = &self.pre_batch;
        let index = storage.get_enumeration_index(key)?;
        Some((storage.read_value(key), index))
    }

    /// The guest's view: every key at its pre-state, except `unproven` keys served as empty.
    fn snapshot(&self, unproven: &HashSet<StorageKey>) -> StorageSnapshot {
        let storage = self
            .dedup_keys
            .union(&self.declared_keys)
            .map(|key| {
                let prestate = if unproven.contains(key) {
                    None
                } else {
                    self.prestate(key)
                };
                (key.hashed_key(), prestate)
            })
            .collect();
        StorageSnapshot::new(storage, self.factory_deps.clone())
    }

    fn rerun<SnapVm>(&self, tx: Transaction, unproven: &HashSet<StorageKey>) -> Option<BatchOutcome>
    where
        SnapVm: VmFactory<StorageView<StorageSnapshot>>,
    {
        let storage = StorageView::new(self.snapshot(unproven)).to_rc_ptr();
        let mut vm = SnapVm::new(self.l1_batch_env.clone(), self.system_env.clone(), storage);
        run_one_tx(&mut vm, tx).map(|(outcome, _)| outcome)
    }
}

fn record<VM: TestedVm>(
    builder: VmTesterBuilder,
    tx: impl FnOnce(&mut Account) -> Transaction,
) -> (Recorded, Transaction) {
    let mut tester = builder.build::<VM>();
    let pre_batch = tester.storage.borrow_mut().inner_mut().clone();
    let tx = tx(&mut tester.rich_accounts[0]);
    let (outcome, dedup_keys) =
        run_one_tx(&mut tester.vm, tx.clone()).expect("bytecode compression failed");

    let cache = tester.storage.borrow().cache();
    let declared_keys = cache
        .read_storage_keys()
        .into_keys()
        .chain(cache.initial_writes().into_keys())
        .collect();
    // Base system contracts are pre-seeded in the VM, so only the rest must come from storage.
    let factory_deps = tester
        .vm
        .decommitted_hashes()
        .into_iter()
        .filter_map(|hash| {
            let hash = u256_to_h256(hash);
            Some((hash, (&pre_batch).load_factory_dep(hash)?))
        })
        .collect();
    let recorded = Recorded {
        outcome,
        dedup_keys: dedup_keys.into_iter().collect(),
        declared_keys,
        pre_batch,
        factory_deps,
        l1_batch_env: tester.l1_batch_env.clone(),
        system_env: tester.system_env.clone(),
    };
    (recorded, tx)
}

/// Asserts `gap` is a real gap, then that proving it reproduces the batch and serving it as empty
/// does not.
fn assert_gap_needs_proof<SnapVm>(recorded: &Recorded, tx: Transaction, gap: StorageKey)
where
    SnapVm: VmFactory<StorageView<StorageSnapshot>>,
{
    assert!(
        recorded.declared_keys.contains(&gap),
        "the VM or the host must have read {gap:?}"
    );
    assert!(
        !recorded.dedup_keys.contains(&gap),
        "{gap:?} must be absent from the deduplicated logs, or `merkle_paths` would prove it"
    );

    let proven = recorded.rerun::<SnapVm>(tx.clone(), &HashSet::new());
    pretty_assertions::assert_eq!(proven.as_ref(), Some(&recorded.outcome));

    let unproven = recorded.rerun::<SnapVm>(tx, &HashSet::from([gap]));
    assert_ne!(
        unproven.as_ref(),
        Some(&recorded.outcome),
        "serving {gap:?} as empty must change the batch; otherwise this test pins nothing"
    );
}

/// A fully rolled-back write leaves no storage log, yet it is priced from the slot's pre-state
/// and the price is carried out through the pubdata counter.
pub(crate) fn test_rolled_back_write_prestate_is_observable<VM, SnapVm>()
where
    VM: TestedVm,
    SnapVm: VmFactory<StorageView<StorageSnapshot>>,
{
    let contract_address = Address::repeat_byte(0x42);
    let slot_s = StorageKey::new(AccountTreeId::new(contract_address), H256::zero());
    let contract = TestContract::rolled_back_write();
    let builder = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(
            contract.bytecode.to_vec(),
            contract_address,
        )])
        .with_storage_slots([(slot_s, H256::from_low_u64_be(7))]);

    let calldata = contract
        .function("outer")
        .encode_input(&[Token::Uint(5.into())])
        .unwrap();
    let (recorded, tx) = record::<VM>(builder, |account| {
        account.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(contract_address),
                calldata,
                value: 0.into(),
                factory_deps: vec![],
            },
            None,
        )
    });
    assert!(
        matches!(recorded.outcome.result, ExecutionResult::Success { .. }),
        "{:?}",
        recorded.outcome.result
    );
    assert_gap_needs_proof::<SnapVm>(&recorded, tx, slot_s);
}

/// The host decides whether to compress a factory dep by reading its `KnownCodesStorage` marker
/// outside the VM. With no gas left after validation, the bootloader never re-reads it.
///
/// Pubdata is free here: otherwise validation keeps back gas for the pubdata spent so far, which
/// leaves enough for the bootloader to read the marker itself.
pub(crate) fn test_known_code_marker_read_outside_vm<VM, SnapVm>()
where
    VM: TestedVm,
    SnapVm: VmFactory<StorageView<StorageSnapshot>>,
{
    let factory_dep = TestContract::counter().bytecode.to_vec();
    let marker = get_known_code_key(&BytecodeHash::for_bytecode(&factory_dep).value());
    let l1_batch_env = L1BatchEnv {
        fee_input: BatchFeeInput::pubdata_independent(50_000_000_000, 250_000_000, 0),
        ..default_l1_batch(L1BatchNumber(1))
    };
    // Built once: loading the system contracts for every probe would dominate the search.
    let storage = VmTesterBuilder::new()
        .with_storage_slots([(marker, H256::from_low_u64_be(1))])
        .build::<VM>()
        .storage
        .borrow_mut()
        .inner_mut()
        .clone();
    let builder = || {
        VmTesterBuilder::new()
            .with_storage(storage.clone())
            .with_l1_batch_env(l1_batch_env.clone())
            .with_execution_mode(TxExecutionMode::VerifyExecute)
            .with_rich_accounts(1)
    };
    let make_tx = |gas_limit: u32| {
        let factory_dep = factory_dep.clone();
        move |account: &mut Account| {
            account.get_l2_tx_for_execute(
                Execute {
                    contract_address: Some(Address::repeat_byte(0x33)),
                    calldata: vec![],
                    value: 0.into(),
                    factory_deps: vec![factory_dep],
                },
                Some(Fee {
                    gas_limit: gas_limit.into(),
                    ..Account::default_fee()
                }),
            )
        }
    };
    // Executes the tx only: a batch whose tx halted cannot be finished.
    let included = |gas_limit: u32| {
        let mut tester = builder().build::<VM>();
        let tx = make_tx(gas_limit)(&mut tester.rich_accounts[0]);
        let (_, result) = tester
            .vm
            .execute_transaction_with_bytecode_compression(tx, true);
        !matches!(result.result, ExecutionResult::Halt { .. })
    };

    // The smallest gas limit the bootloader still includes: validation passes, nothing is left.
    let (mut lo, mut hi) = (0_u32, 10_000_000_u32);
    assert!(included(hi), "the upper bound must be included");
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if included(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }

    let (recorded, tx) = record::<VM>(builder(), make_tx(hi));
    assert_gap_needs_proof::<SnapVm>(&recorded, tx, marker);
}
