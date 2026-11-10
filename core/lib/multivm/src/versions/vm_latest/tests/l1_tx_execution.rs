use assert_matches::assert_matches;
use zksync_test_contracts::{TestContract, TxType};
use zksync_types::{
    protocol_upgrade::ProtocolUpgradeTxCommonData, Execute, ExecuteTransactionCommon, Transaction,
    CONTRACT_FORCE_DEPLOYER_ADDRESS, H256, REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE, U256,
};

use crate::{
    interface::{ExecutionResult, Halt, InspectExecutionMode, TxExecutionMode, VmInterfaceExt},
    versions::testonly::{
        l1_tx_execution::{
            test_l1_tx_execution, test_l1_tx_execution_gas_estimation_with_low_gas,
            test_l1_tx_execution_high_gas_limit, test_l1_tx_force_fail,
        },
        VmTesterBuilder,
    },
    vm_latest::{types::TransactionData, HistoryEnabled, Vm},
};

#[test]
fn l1_tx_execution() {
    test_l1_tx_execution::<Vm<_, HistoryEnabled>>();
}

#[test]
fn l1_tx_execution_high_gas_limit() {
    test_l1_tx_execution_high_gas_limit::<Vm<_, HistoryEnabled>>();
}

#[test]
fn l1_tx_execution_gas_estimation_with_low_gas() {
    test_l1_tx_execution_gas_estimation_with_low_gas::<Vm<_, HistoryEnabled>>();
}

#[test]
fn l1_tx_force_fail() {
    test_l1_tx_force_fail::<Vm<_, HistoryEnabled>>();
}

/// The flag only exists on `L1TxCommonData`, so these bypass `Transaction` and set it on the
/// `TransactionData` directly, the way a buggy server would.
fn assert_force_fail_rejected(tx: Transaction, expected_msg: &str) {
    let mut vm = VmTesterBuilder::new()
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_l1_base_token_minting(H256::repeat_byte(0x11))
        .with_rich_accounts(1)
        .build::<Vm<_, HistoryEnabled>>();

    let mut tx = TransactionData::new(tx, false);
    tx.force_fail = true;
    let overhead = tx.overhead_gas();
    vm.vm.push_raw_transaction(tx, overhead, 0, true);

    let res = vm.vm.execute(InspectExecutionMode::OneTx);
    assert_matches!(
        &res.result,
        ExecutionResult::Halt { reason: Halt::UnexpectedVMBehavior(msg) } if msg.contains(expected_msg)
    );
}

#[test]
fn force_fail_on_upgrade_tx_is_rejected() {
    let tx = Transaction {
        common_data: ExecuteTransactionCommon::ProtocolUpgrade(ProtocolUpgradeTxCommonData {
            sender: CONTRACT_FORCE_DEPLOYER_ADDRESS,
            gas_limit: U256::from(200_000_000u32),
            gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
            ..Default::default()
        }),
        execute: Execute {
            contract_address: Some(CONTRACT_FORCE_DEPLOYER_ADDRESS),
            calldata: vec![],
            factory_deps: vec![],
            value: U256::zero(),
        },
        received_timestamp_ms: 0,
        raw_bytes: None,
    };
    assert_force_fail_rejected(tx, "forceFail on upgrade tx");
}

#[test]
fn force_fail_on_l2_tx_is_rejected() {
    let mut vm = VmTesterBuilder::new()
        .with_rich_accounts(1)
        .build::<Vm<_, HistoryEnabled>>();
    let tx = vm.rich_accounts[0]
        .get_deploy_tx(TestContract::counter().bytecode, None, TxType::L2)
        .tx;
    assert_force_fail_rejected(tx, "forceFail on L2 tx");
}
