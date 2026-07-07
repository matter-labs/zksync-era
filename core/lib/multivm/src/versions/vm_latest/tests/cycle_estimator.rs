use std::sync::Arc;

use once_cell::sync::OnceCell;
use zksync_era_airbender_cycles_estimator::{estimate_from_features, BatchContext, FeatureId};
use zksync_types::{Address, Execute, U256};

use super::TestedLatestVm;
use crate::{
    interface::{InspectExecutionMode, TxExecutionMode, VmInterface},
    tracers::CycleFeatureTracer,
    versions::testonly::VmTesterBuilder,
    vm_latest::{constants::BATCH_COMPUTATIONAL_GAS_LIMIT, ToTracerPointer},
};

/// Drives a real legacy-VM transfer with the [`CycleFeatureTracer`] attached and
/// checks that (a) it fills the shared [`FeatureVector`] with the opcode/complexity
/// features the cost model expects, and (b) the reused VM-agnostic estimator turns
/// them into a positive, reliable cycle estimate.
#[test]
fn cycle_feature_tracer_collects_features_and_estimates() {
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .with_bootloader_gas_limit(BATCH_COMPUTATIONAL_GAS_LIMIT)
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .build::<TestedLatestVm>();

    let account = &mut vm.rich_accounts[0];
    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: Some(Address::repeat_byte(1)),
            calldata: Vec::new(),
            value: U256::from(1u8),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let result = Arc::new(OnceCell::default());
    let tracer = CycleFeatureTracer::new(result.clone());
    let tracer_ptr = tracer.into_tracer_pointer();
    let res = vm
        .vm
        .inspect(&mut tracer_ptr.into(), InspectExecutionMode::OneTx);
    assert!(!res.result.is_failed(), "{res:#?}");

    let features = Arc::try_unwrap(result)
        .unwrap()
        .take()
        .expect("tracer must publish its feature vector after execution");

    // A bootloader-driven transfer executes plenty of ordinary opcodes, at least
    // one far call (into the callee), and touches storage.
    assert!(
        features.get(FeatureId::RichAddressingOp) > 0,
        "expected rich-addressing opcodes to be counted"
    );
    assert!(
        features.get(FeatureId::FarCall) > 0,
        "expected at least one far call"
    );
    assert!(
        features.get(FeatureId::StorageApplication) > 0,
        "expected storage applications from the transfer"
    );

    // Feed the traced features + batch-level scalars into the SAME cost model the
    // fast VM uses. A plain transfer prices reliably (no unpriced precompiles).
    let ctx = BatchContext {
        transaction_count: 1,
        merkle_leaf_count: features.get(FeatureId::StorageApplication),
        storage_key_count: features.get(FeatureId::StorageApplication),
        used_bytecode_bytes: 0,
        used_bytecode_count: 0,
    };
    let estimate =
        estimate_from_features(features, /*pubdata*/ 0, /*state_diffs*/ 0, &ctx);
    assert!(estimate.total > 0, "estimate must be positive");
    assert!(
        estimate.is_reliable(),
        "a plain transfer uses no unpriced precompiles: {:?}",
        estimate.unpriced
    );
    assert!(estimate.phases.contains_key("vm_execution"));
}
