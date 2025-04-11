use super::TestedVm;

pub(crate) fn test_trivial_test_storage_logs<VM: TestedVm>() {
    // let (vm, test_data) = setup_v26_unsafe_deposits_detection::<VM>();
    // assert_eq!(test_data, get_test_data_second_option());

    // let storage_ptr = vm.storage.clone();
    // let borrowed = storage_ptr.borrow();

    // let expected = remove_irrelevant_keys(borrowed.modified_storage_keys().clone());
    // let found = remove_irrelevant_keys(trivial_test_storage_logs());

    // assert_eq!(expected, found);
}

pub(crate) fn test_post_bridging_test_storage_logs<VM: TestedVm>() {
    // let (mut vm, test_data) = setup_v26_unsafe_deposits_detection::<VM>();
    // assert_eq!(test_data, get_test_data());

    // let l1_tx_new_deposit = Transaction {
    //     common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
    //         sender: test_data.l1_aliased_shared_bridge,
    //         gas_limit: 200_000_000u32.into(),
    //         gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
    //         ..Default::default()
    //     }),
    //     execute: Execute {
    //         contract_address: Some(test_data.l2_legacy_shared_bridge_address),
    //         calldata: encode_legacy_finalize_deposit(test_data.l1_token_address),
    //         value: Default::default(),
    //         factory_deps: vec![],
    //     },
    //     received_timestamp_ms: 0,
    //     raw_bytes: None,
    // };

    // vm.execute_tx_and_verify(TransactionTestInfo::new_processed(l1_tx_new_deposit, false));

    // let storage_ptr = vm.storage.clone();
    // let borrowed = storage_ptr.borrow();

    // let expected = remove_irrelevant_keys(borrowed.modified_storage_keys().clone());
    // let found: HashMap<StorageKey, H256> =
    //     remove_irrelevant_keys(post_bridging_test_storage_logs());

    // assert_eq!(expected, found);
}

pub(crate) fn test_post_registration_storage_logs<VM: TestedVm>() {
    // let (mut vm, test_data) = setup_v26_unsafe_deposits_detection::<VM>();
    // assert_eq!(test_data, get_test_data());

    // let l1_tx_regisrtation = Transaction {
    //     common_data: ExecuteTransactionCommon::L1(L1TxCommonData {
    //         sender: test_data.l1_aliased_shared_bridge,
    //         gas_limit: 200_000_000u32.into(),
    //         gas_per_pubdata_limit: REQUIRED_L1_TO_L2_GAS_PER_PUBDATA_BYTE.into(),
    //         ..Default::default()
    //     }),
    //     execute: Execute {
    //         contract_address: Some(L2_NATIVE_TOKEN_VAULT_ADDRESS),
    //         calldata: encode_regisration(test_data.l2_token_address),
    //         value: Default::default(),
    //         factory_deps: vec![],
    //     },
    //     received_timestamp_ms: 0,
    //     raw_bytes: None,
    // };

    // vm.execute_tx_and_verify(TransactionTestInfo::new_processed(
    //     l1_tx_regisrtation,
    //     false,
    // ));

    // let storage_ptr = vm.storage.clone();
    // let borrowed = storage_ptr.borrow();

    // let expected = remove_irrelevant_keys(borrowed.modified_storage_keys().clone());
    // let found: HashMap<StorageKey, H256> =
    //     remove_irrelevant_keys(post_registration_test_storage_logs());

    // assert_eq!(expected, found);
}
