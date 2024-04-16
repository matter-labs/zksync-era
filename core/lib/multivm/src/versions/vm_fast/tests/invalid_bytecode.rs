use zksync_types::H256;
use zksync_utils::h256_to_u256;

use crate::vm_latest::tests::tester::VmTesterBuilder;
use crate::vm_latest::types::inputs::system_env::TxExecutionMode;
use crate::vm_latest::{HistoryEnabled, TxRevertReason};

// TODO this test requires a lot of hacks for bypassing the bytecode checks in the VM.
// Port it later, it's not significant. for now

#[test]
fn test_invalid_bytecode() {
    let mut vm_builder = VmTesterBuilder::new(HistoryEnabled)
        .with_in_memory_storage()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1);
    let mut storage = vm_builder.take_storage();
    let mut vm = vm_builder.build(&mut storage);

    let block_gas_per_pubdata = vm_test_env
        .block_context
        .context
        .block_gas_price_per_pubdata();

    let mut test_vm_with_custom_bytecode_hash =
        |bytecode_hash: H256, expected_revert_reason: Option<TxRevertReason>| {
            let mut oracle_tools =
                OracleTools::new(vm_test_env.storage_ptr.as_mut(), HistoryEnabled);

            let (encoded_tx, predefined_overhead) = get_l1_tx_with_custom_bytecode_hash(
                h256_to_u256(bytecode_hash),
                block_gas_per_pubdata as u32,
            );

            run_vm_with_custom_factory_deps(
                &mut oracle_tools,
                vm_test_env.block_context.context,
                &vm_test_env.block_properties,
                encoded_tx,
                predefined_overhead,
                expected_revert_reason,
            );
        };

    let failed_to_mark_factory_deps = |msg: &str, data: Vec<u8>| {
        TxRevertReason::FailedToMarkFactoryDependencies(VmRevertReason::General {
            msg: msg.to_string(),
            data,
        })
    };

    // Here we provide the correctly-formatted bytecode hash of
    // odd length, so it should work.
    test_vm_with_custom_bytecode_hash(
        H256([
            1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ]),
        None,
    );

    // Here we provide correctly formatted bytecode of even length, so
    // it should fail.
    test_vm_with_custom_bytecode_hash(
        H256([
            1, 0, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ]),
        Some(failed_to_mark_factory_deps(
            "Code length in words must be odd",
            vec![
                8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 67, 111, 100, 101, 32, 108, 101, 110,
                103, 116, 104, 32, 105, 110, 32, 119, 111, 114, 100, 115, 32, 109, 117, 115, 116,
                32, 98, 101, 32, 111, 100, 100,
            ],
        )),
    );

    // Here we provide incorrectly formatted bytecode of odd length, so
    // it should fail.
    test_vm_with_custom_bytecode_hash(
        H256([
            1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ]),
        Some(failed_to_mark_factory_deps(
            "Incorrectly formatted bytecodeHash",
            vec![
                8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 73, 110, 99, 111, 114, 114, 101, 99,
                116, 108, 121, 32, 102, 111, 114, 109, 97, 116, 116, 101, 100, 32, 98, 121, 116,
                101, 99, 111, 100, 101, 72, 97, 115, 104, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        )),
    );

    // Here we provide incorrectly formatted bytecode of odd length, so
    // it should fail.
    test_vm_with_custom_bytecode_hash(
        H256([
            2, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ]),
        Some(failed_to_mark_factory_deps(
            "Incorrectly formatted bytecodeHash",
            vec![
                8, 195, 121, 160, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 34, 73, 110, 99, 111, 114, 114, 101, 99,
                116, 108, 121, 32, 102, 111, 114, 109, 97, 116, 116, 101, 100, 32, 98, 121, 116,
                101, 99, 111, 100, 101, 72, 97, 115, 104, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        )),
    );
}
