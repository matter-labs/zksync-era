//! Cycle-estimator tracer tests. These drive *real batches* — several
//! transactions executed one after another through the VM with the
//! [`CycleFeatureTracer`](crate::tracers::CycleFeatureTracer) attached — collect
//! the opcode/complexity features the tracer observes, and then feed them
//! through the SAME calibrated Airbender cost model the fast VM uses to check the
//! cycle arithmetic produces a sane, positive estimate.
//!
//! Like the call-tracer tests, they keep no fixtures: they assert relationships
//! (features are present, the estimate is positive/reliable, more work costs more
//! cycles) rather than exact cycle counts, which unrelated system-contract
//! changes would otherwise invalidate.

use ethabi::Token;
use zksync_test_contracts::TestContract;
use zksync_types::{
    bytecode::BytecodeHash, get_known_code_key, u256_to_h256, web3::keccak256, Address, Execute,
    U256,
};

use super::{
    get_empty_storage, ContractToDeploy, TestedVmWithCycleTracer, VmTester, VmTesterBuilder,
};
use crate::{
    interface::TxExecutionMode,
    tracers::cycle_estimator::{estimate_from_features, CycleEstimate, FeatureId, FeatureVector},
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};

/// Address the counter test contract is pre-deployed at.
const COUNTER_ADDRESS: Address = Address::repeat_byte(0xC0);

/// `increment(6)` calldata for the counter contract (same call the call-tracer
/// `test_basic_behavior` uses); writes storage, so it exercises the storage and
/// far-call features.
fn increment_calldata() -> Vec<u8> {
    hex::decode("7cf5dab00000000000000000000000000000000000000000000000000000000000000006").unwrap()
}

/// Sum the per-transaction feature vector `tx` into the running batch vector.
fn accumulate(batch: &mut FeatureVector, tx: &FeatureVector) {
    for (&id, &count) in &tx.counts {
        batch.add(id, count);
    }
}

/// A sequencer prices a batch from features it collects while executing it plus a
/// few batch-level scalars it already holds. At sequencing time the merkle
/// witness does not exist yet, so the number of distinct storage applications is
/// the estimate of the leaves the tree will witness.
fn estimate_batch(features: FeatureVector, tx_count: u64) -> CycleEstimate {
    let merkle_leaf_count = features.get(FeatureId::StorageApplication);
    estimate_from_features(
        features,
        /* pubdata */ 0,
        /* state_diffs */ 0,
        tx_count,
        merkle_leaf_count,
    )
}

/// Build a tester with the counter contract pre-deployed and a single rich
/// account.
fn build_tester<VM: TestedVmWithCycleTracer>() -> VmTester<VM> {
    VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_custom_contracts(vec![ContractToDeploy::new(
            TestContract::counter().bytecode.to_vec(),
            COUNTER_ADDRESS,
        )])
        .build()
}

/// Execute a batch of `counter_calls` counter increments followed by one plain
/// value transfer, tracing each transaction and summing the collected features.
/// Returns the accumulated batch feature vector and the transaction count.
fn run_batch<VM: TestedVmWithCycleTracer>(
    vm: &mut VmTester<VM>,
    counter_calls: usize,
) -> (FeatureVector, u64) {
    let mut batch = FeatureVector::default();
    let mut tx_count = 0u64;

    for _ in 0..counter_calls {
        let account = &mut vm.rich_accounts[0];
        let tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(COUNTER_ADDRESS),
                calldata: increment_calldata(),
                value: U256::zero(),
                factory_deps: vec![],
            },
            None,
        );
        vm.vm.push_transaction(tx);
        let (res, features) = vm.vm.inspect_with_cycle_tracer();
        assert!(
            !res.result.is_failed(),
            "counter call failed: {:#?}",
            res.result
        );
        accumulate(&mut batch, &features);
        tx_count += 1;
    }

    // A plain transfer: no calldata, some value — a real tx that still touches
    // storage (balance updates) and makes far calls.
    let account = &mut vm.rich_accounts[0];
    let transfer = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(Address::repeat_byte(0x11)),
            calldata: vec![],
            value: U256::from(1u8),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(transfer);
    let (res, features) = vm.vm.inspect_with_cycle_tracer();
    assert!(
        !res.result.is_failed(),
        "transfer failed: {:#?}",
        res.result
    );
    accumulate(&mut batch, &features);
    tx_count += 1;

    (batch, tx_count)
}

/// Drives a real multi-transaction batch with the [`CycleFeatureTracer`] attached and
/// checks that it fills the [`FeatureVector`] with the features the cost table prices,
/// and that the estimator turns them into a positive, reliable estimate.
pub(crate) fn test_basic_behavior<VM: TestedVmWithCycleTracer>() {
    let mut vm = build_tester::<VM>();
    let (features, tx_count) = run_batch(&mut vm, 2);

    // A batch of contract calls + a transfer executes plenty of ordinary opcodes,
    // makes far calls into the callees, and applies storage writes.
    for (id, name) in [
        (FeatureId::ArithCheapOp, "cheap"),
        (FeatureId::ArithShiftOp, "shift"),
        (FeatureId::ArithMulOp, "mul"),
        (FeatureId::ArithDivOp, "div"),
        (FeatureId::ArithPtrOp, "pointer"),
    ] {
        // Each arithmetic class is filled separately: an axis the tracer never fills
        // reads as work the batch never did.
        assert!(
            features.get(id) > 0,
            "expected {name} arithmetic opcodes to be counted"
        );
    }
    assert!(
        features.get(FeatureId::FarCall) > 0,
        "expected at least one far call"
    );
    assert!(
        features.get(FeatureId::StorageApplication) > 0,
        "expected storage applications from the batch"
    );

    // Feed the traced features + batch-level scalars into the SAME cost model the
    // fast VM uses. A counter/transfer batch prices reliably (no unpriced
    // precompiles).
    let estimate = estimate_batch(features, tx_count);

    assert!(
        estimate.total > 0,
        "estimate must be positive: {estimate:?}"
    );
    assert!(
        estimate.is_reliable(),
        "a counter/transfer batch prices with no unpriced axis and a filled trace: {estimate:?}"
    );
    assert!(
        estimate.marginal < estimate.total,
        "the per-batch base belongs to `total` only: {estimate:?}"
    );

    // The conservative (safety-margined) value never shrinks below `total`.
    assert!(estimate.conservative() >= estimate.total);
}

/// The estimate is monotone in the work actually executed: a bigger batch (more
/// transactions ⇒ more opcodes, more storage applications) collects strictly more
/// features and therefore never prices below a smaller one. This exercises the
/// end-to-end arithmetic, not just its presence.
pub(crate) fn test_estimate_scales_with_batch_size<VM: TestedVmWithCycleTracer>() {
    let mut small_vm = build_tester::<VM>();
    let (small_features, small_txs) = run_batch(&mut small_vm, 1);

    let mut large_vm = build_tester::<VM>();
    let (large_features, large_txs) = run_batch(&mut large_vm, 5);

    // The larger batch really did more work.
    assert!(large_txs > small_txs);
    assert!(
        large_features.get(FeatureId::ArithCheapOp) > small_features.get(FeatureId::ArithCheapOp),
        "a larger batch must execute more opcodes"
    );
    assert!(
        large_features.get(FeatureId::StorageApplication)
            >= small_features.get(FeatureId::StorageApplication),
        "a larger batch must not apply fewer storage slots"
    );

    let small = estimate_batch(small_features, small_txs);
    let large = estimate_batch(large_features, large_txs);

    assert!(small.total > 0 && large.total > 0);
    assert!(
        large.total > small.total,
        "more work must cost more cycles: {} !> {}",
        large.total,
        small.total
    );
}

/// A DECOMMIT of a hash already decommitted in this VM run redoes work that no counter
/// reports and whose ergs are refunded, so it is priced per call — 2.01e6 cycles against
/// 7,123 per 64 bytes for a fresh one. Nothing else in the estimate distinguishes the two,
/// so both producers derive the repeat from the absence of a fresh decommit, and this
/// checks that they do: the same `codeOracle` call twice, fresh then repeat.
pub(crate) fn test_repeat_decommit_is_counted<VM: TestedVmWithCycleTracer>() {
    let precompiles_contract_address = Address::repeat_byte(1);
    let decommitted_bytecode = TestContract::counter().bytecode;
    let decommitted_bytecode_hash = BytecodeHash::for_bytecode(decommitted_bytecode).value();

    let mut storage = get_empty_storage();
    storage.set_value(
        get_known_code_key(&decommitted_bytecode_hash),
        u256_to_h256(U256::one()),
    );
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_rich_accounts(1)
        .with_custom_contracts(vec![ContractToDeploy::new(
            TestContract::precompiles_test().bytecode.to_vec(),
            precompiles_contract_address,
        )])
        .with_storage(storage)
        .build::<VM>();
    vm.vm.insert_bytecodes(&[decommitted_bytecode]);

    let call_code_oracle = TestContract::precompiles_test().function("callCodeOracle");
    let calldata = call_code_oracle
        .encode_input(&[
            Token::FixedBytes(decommitted_bytecode_hash.0.to_vec()),
            Token::FixedBytes(keccak256(decommitted_bytecode).to_vec()),
        ])
        .unwrap();

    let mut decommits = Vec::with_capacity(2);
    for _ in 0..2 {
        let account = &mut vm.rich_accounts[0];
        let tx = account.get_l2_tx_for_execute(
            Execute {
                contract_address: Some(precompiles_contract_address),
                calldata: calldata.clone(),
                value: U256::zero(),
                factory_deps: vec![],
            },
            None,
        );
        vm.vm.push_transaction(tx);
        let (res, features) = vm.vm.inspect_with_cycle_tracer();
        assert!(
            !res.result.is_failed(),
            "code oracle call failed: {:#?}",
            res.result
        );
        assert!(
            features.get(FeatureId::Decommit) > 0,
            "the code oracle executes a DECOMMIT"
        );
        decommits.push(features.get(FeatureId::DecommitRepeat));
    }

    assert_eq!(
        decommits[0], 0,
        "the first decommit of a bytecode is fresh, not a repeat"
    );
    assert!(
        decommits[1] > 0,
        "decommitting the same bytecode again must be counted as a repeat"
    );
}
