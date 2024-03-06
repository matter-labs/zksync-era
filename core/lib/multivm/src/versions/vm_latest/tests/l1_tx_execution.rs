use ethabi::Token;
use zksync_contracts::l1_messenger_contract;
use zksync_system_constants::{BOOTLOADER_ADDRESS, L1_MESSENGER_ADDRESS};
use zksync_types::{
    get_code_key, get_known_code_key,
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    storage_writes_deduplicator::StorageWritesDeduplicator,
    Execute, ExecuteTransactionCommon, U256,
};
use zksync_utils::u256_to_h256;

use crate::{
    interface::{TxExecutionMode, VmExecutionMode, VmInterface},
    vm_latest::{
        tests::{
            tester::{TxType, VmTesterBuilder},
            utils::{read_test_contract, verify_required_storage, BASE_SYSTEM_CONTRACTS},
        },
        types::internals::TransactionData,
        HistoryEnabled,
    },
};

#[test]
fn test_l1_tx_execution() {
    // In this test, we try to execute a contract deployment from L1
    // Here instead of marking code hash via the bootloader means, we will be
    // using L1->L2 communication, the same it would likely be done during the priority mode.

    // There are always at least 9 initial writes here, because we pay fees from l1:
    // - `totalSupply` of ETH token
    // - balance of the refund recipient
    // - balance of the bootloader
    // - `tx_rolling` hash
    // - `gasPerPubdataByte`
    // - `basePubdataSpent`
    // - rolling hash of L2->L1 logs
    // - transaction number in block counter
    // - L2->L1 log counter in `L1Messenger`

    // TODO(PLA-537): right now we are using 5 slots instead of 9 due to 0 fee for transaction.
    let basic_initial_writes = 5;

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let contract_code = read_test_contract();
    let account = &mut vm.rich_accounts[0];
    let deploy_tx = account.get_deploy_tx(&contract_code, None, TxType::L1 { serial_id: 1 });
    let tx_data: TransactionData = deploy_tx.tx.clone().into();

    let required_l2_to_l1_logs: Vec<_> = vec![L2ToL1Log {
        shard_id: 0,
        is_service: true,
        tx_number_in_block: 0,
        sender: BOOTLOADER_ADDRESS,
        key: tx_data.tx_hash(0.into()),
        value: u256_to_h256(U256::from(1u32)),
    }]
    .into_iter()
    .map(UserL2ToL1Log)
    .collect();

    vm.vm.push_transaction(deploy_tx.tx.clone());

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    // The code hash of the deployed contract should be marked as republished.
    let known_codes_key = get_known_code_key(&deploy_tx.bytecode_hash);

    // The contract should be deployed successfully.
    let account_code_key = get_code_key(&deploy_tx.address);

    let expected_slots = vec![
        (u256_to_h256(U256::from(1u32)), known_codes_key),
        (deploy_tx.bytecode_hash, account_code_key),
    ];
    assert!(!res.result.is_failed());

    verify_required_storage(&vm.vm.state, expected_slots);

    assert_eq!(res.logs.user_l2_to_l1_logs, required_l2_to_l1_logs);

    let tx = account.get_test_contract_transaction(
        deploy_tx.address,
        true,
        None,
        false,
        TxType::L1 { serial_id: 0 },
    );
    vm.vm.push_transaction(tx);
    let res = vm.vm.execute(VmExecutionMode::OneTx);
    let storage_logs = res.logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);

    // Tx panicked
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 0);

    let tx = account.get_test_contract_transaction(
        deploy_tx.address,
        false,
        None,
        false,
        TxType::L1 { serial_id: 0 },
    );
    vm.vm.push_transaction(tx.clone());
    let res = vm.vm.execute(VmExecutionMode::OneTx);
    let storage_logs = res.logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);
    // We changed one slot inside contract. However, the rewrite of the `basePubdataSpent` didn't happen, since it was the same
    // as the start of the previous tx. Thus we have `+1` slot for the changed counter and `-1` slot for base pubdata spent
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 0);

    // No repeated writes
    let repeated_writes = res.repeated_storage_writes;
    assert_eq!(res.repeated_storage_writes, 0);

    vm.vm.push_transaction(tx);
    let storage_logs = vm.vm.execute(VmExecutionMode::OneTx).logs.storage_logs;
    let res = StorageWritesDeduplicator::apply_on_empty_state(&storage_logs);
    // We do the same storage write, it will be deduplicated, so still 4 initial write and 0 repeated.
    // But now the base pubdata spent has changed too.
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 1);
    assert_eq!(res.repeated_storage_writes, repeated_writes);

    let tx = account.get_test_contract_transaction(
        deploy_tx.address,
        false,
        Some(10.into()),
        false,
        TxType::L1 { serial_id: 1 },
    );
    vm.vm.push_transaction(tx);
    let result = vm.vm.execute(VmExecutionMode::OneTx);
    // Method is not payable tx should fail
    assert!(result.result.is_failed(), "The transaction should fail");

    let res = StorageWritesDeduplicator::apply_on_empty_state(&result.logs.storage_logs);
    // There are only basic initial writes
    assert_eq!(res.initial_storage_writes - basic_initial_writes, 1);
}

#[test]
fn test_l1_tx_execution_high_gas_limit() {
    // In this test, we try to execute an L1->L2 transaction with a high gas limit.
    // Usually priority transactions with dangerously gas limit should even pass the checks on the L1,
    // however, they might pass during the transition period to the new fee model, so we check that we can safely process those.

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let account = &mut vm.rich_accounts[0];

    let l1_messenger = l1_messenger_contract();

    let contract_function = l1_messenger.function("sendToL1").unwrap();
    let params = [
        // Even a message of size 100k should not be able to be sent by a priority transaction
        Token::Bytes(vec![0u8; 100_000]),
    ];
    let calldata = contract_function.encode_input(&params).unwrap();

    let mut tx = account.get_l1_tx(
        Execute {
            contract_address: L1_MESSENGER_ADDRESS,
            value: 0.into(),
            factory_deps: None,
            calldata,
        },
        0,
    );

    if let ExecuteTransactionCommon::L1(data) = &mut tx.common_data {
        // Using some large gas limit
        data.gas_limit = 300_000_000.into();
    } else {
        unreachable!()
    };

    vm.vm.push_transaction(tx);

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    assert!(res.result.is_failed(), "The transaction should've failed");
}

/*


(base) stas@Stanislavs-MacBook-Pro 2024-03-zksync-code4rena % git diff main | grep TODO -C 5
+++ b/docs/Smart contract Section/System contracts bootloader description.md
@@ -75,6 +75,8 @@ There is no need to copy returned data if the B returns a slice of the returndat

 Note, that you can *not* use the pointer that you received via calldata as returndata (i.e. return it at the end of the execution frame). Otherwise, it would be possible that returndata points to the memory slice of the active frame and allow editing the `returndata`. It means that in the examples above, C could not return a slice of its calldata without memory copying.

+Note, that the rule above is implemented by the principle "it is not possible to return a slice of data with memory page id lower than the memory page id of the current heap", since a memory page with smaller id could only be created before the call. That's why a user contract can usually safely return a slice of previously returned returndata (since it is guaranteed to have a higher memory page id). However, system contracts have an exeption from the rule above. It is needed in particular to the correct functionality of the CodeOracle system contract (TODO: link). So the rule of thumb is that returndata from `CodeOracle` should never be passed along.
+
 Some of these memory optimizations can be seen utilized in the [EfficientCall](https://github.com/code-423n4/2023-10-zksync/blob/main/code/system-contracts/contracts/libraries/EfficientCall.sol) library that allows to perform a call while reusing the slice of calldata that the frame already has, without memory copying.

 ### Returndata & precompiles
@@ -108,10 +110,11 @@ These opcodes are allowed only for contracts in kernel space (i.e. system contr
--

-`BOOTLOADER_MEMORY_FOR_TXS` (*BM*) — The size of the bootloader memory that is used for transaction encoding (i.e. excluding the constant space, preallocated for other purposes).
+# How we charge for pubdata

-`GUARANTEED_PUBDATA_PER_TX` (*PG*) — The guaranteed number of pubdata that should be possible to pay for in one zkSync batch. This is a number that should be enough for most reasonable cases.
+Let's define `gas_per_pubdata` as the number of L2 gas units the user should pay for each byte of pubdata on L1. This price is set as `pubdataPriceETH / l2GasPrice`, where `pubdataPriceETH` is the price of each byte of pubdata in ETH (the derivation of this value will be described below: TODO link) and `l2GasPrice` is the price of each unit of L2 gas in ETH.

-### Derived constants
+zkSync Era is a state diff based rollup. It means that it is not possible to know how much pubdata will a transaction take prior to its execution. We *could* charge for pubdata the following way: henever a user does an operation that emits pubdata (writes to storage, publishes and L2->L1 message, etc), we charge `pubdata_bytes_published * gas_per_pubdata` directly from the context of the execution.

-Some of the constants are derived from the system constants above:
--
-Base = max(E_f, \lceil \frac{L1_P * L1_{PUB}}{EP_{max}} \rceil)
-$$
+If a user really needs to limit the amount of gas that the subcall takes, all the subcalls should be routed through a special system contract, that will guarantee that the total cost of the subcall wont be larger than the gas provided (by reverting if needed).

-This is the base fee that will be always returned from the API via `eth_gasGasPrice`.
+TODO: add a link to this system contract

-### Calculating overhead for a transaction
+### 1. Case of when a malicious contract consumes a large, but processable amount of pubdata**

-Let’s define by *tx*.*actualGasLimit* as the actual gasLimit that is to be used for processing of the transaction (including the intrinsic costs). In this case, we will use the following formulas for calculating the upfront payment for the overhead:
--
+Note, however, than it means that the total under high L1 gas prices `gasLimit` may be larger than `u32::MAX` and it is recommended than no more than `2^20` bytes of pubdata can be published within a transaction.

-$$
-O_{op} ≤ overhead_gas(tx)
-$$
+TODO: implement the logic above

-for the $tx.bodyGasLimit$ we use the $tx.bodyGasLimit$ = $tx.gasLimit − O_{op}$.
+### Recommended calculation of `FAIR_L2_GAS_PRICE`/`FAIR_PUBDATA_PRICE`

-There are a few approaches that could be taken here:

*/
