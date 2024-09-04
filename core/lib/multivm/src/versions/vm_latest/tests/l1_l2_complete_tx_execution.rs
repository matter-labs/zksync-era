use ethabi::{encode, Contract, Token};
use zksync_basic_types::{web3::keccak256, Address, H256};
use zksync_system_constants::{
    L2_ASSET_ROUTER_ADDRESS, SETTLEMENT_LAYER_RELAY_SENDER, SHARED_BRIDGE_ETHER_TOKEN_ADDRESS,
};
use zksync_test_account::Account;
use zksync_types::{
    diamond::{compile_initial_cut_hash, facet_cut, Action, ChainCreationParams, VerifierParams},
    mailbox::BridgeHubRequestL2TransactionOnGateway,
    U256,
};
use zksync_utils::u256_to_h256;

use crate::{
    interface::TxExecutionMode,
    vm_latest::{
        tests::{
            tester::{VmTester, VmTesterBuilder},
            utils::{
                deploy_and_verify_contract, read_admin_facet, read_bridgehub, read_diamond,
                read_diamond_init, read_diamond_proxy, read_mailbox_facet, read_message_rooot,
                read_stm, read_transparent_proxy, send_l2_tx_and_verify, send_prank_tx_and_verify,
                undo_l1_to_l2_alias, BASE_SYSTEM_CONTRACTS,
            },
        },
        HistoryEnabled,
    },
};

fn prepare_environment_and_deploy_contracts(
    vm: &mut VmTester<HistoryEnabled>,
    deploy_account_address: Address,
) -> (Address, Contract) {
    let mut deploy_nonce = 0;

    // Deploy Bridgehub

    let (bridgehub_contract_bytecode, _bridgehub_contract) = read_bridgehub();
    let l1_chain_id = Token::Uint(0.into()); // Chain ID hasn't been set during vm tester construction, as it is not affecting the test environment
    let max_number_of_hyperchains = U256::from(100);
    let constructor_data = &[
        l1_chain_id.clone(),
        Token::Address(deploy_account_address),
        Token::Uint(max_number_of_hyperchains),
    ];
    let bridgehub_address = deploy_and_verify_contract(
        vm,
        0,
        &bridgehub_contract_bytecode,
        Some(constructor_data),
        &mut deploy_nonce,
    );

    // Deploy Message Root

    let message_root_contract_bytecode = read_message_rooot();
    let max_number_of_hyperchains = U256::from(100);
    let constructor_data = &[Token::Address(bridgehub_address)];
    let message_root_address = deploy_and_verify_contract(
        vm,
        0,
        &message_root_contract_bytecode,
        Some(constructor_data),
        &mut deploy_nonce,
    );

    // Deploy STM

    let (stm_contract_code, stm_contract) = read_stm();
    // Set the constructor data to L2 bridgehub address and max number of hyperchains to 100
    let constructor_data = &[
        Token::Address(bridgehub_address),
        Token::Uint(max_number_of_hyperchains),
    ];

    let stm_address = deploy_and_verify_contract(
        vm,
        0,
        &stm_contract_code,
        Some(constructor_data),
        &mut deploy_nonce,
    );

    // Deploy Mailbox Facet

    let (mailbox_facet_contract_code, mailbox_facet_contract) = read_mailbox_facet();
    let era_chain_id = Token::Uint(1.into());
    let constructor_data = &[era_chain_id, l1_chain_id.clone()];

    let mailbox_facet_address = deploy_and_verify_contract(
        vm,
        0,
        &mailbox_facet_contract_code,
        Some(constructor_data),
        &mut deploy_nonce,
    );

    // Deploy Admin Facet

    let (admin_facet_contract_code, admin_facet_contract) = read_admin_facet();
    let constructor_data = &[l1_chain_id.clone()];
    let admin_facet_address = deploy_and_verify_contract(
        vm,
        0,
        &admin_facet_contract_code,
        Some(constructor_data),
        &mut deploy_nonce,
    );

    // Deploy Diamond Init

    let (diamond_contract_code, _diamond_contract) = read_diamond();
    let (diamond_init_contract_code, _diamond_init_contract) = read_diamond_init();
    let (diamond_proxy_contract_code, _diamond_proxy_contract) = read_diamond_proxy();
    let diamond_init_address =
        deploy_and_verify_contract(vm, 0, &diamond_init_contract_code, None, &mut deploy_nonce);

    // Collect Data to Initialize STM

    let verifier_params = VerifierParams {
        recursion_node_level_vk_hash: H256::zero(),
        recursion_leaf_level_vk_hash: H256::zero(),
        recursion_circuits_set_vks_hash: H256::zero(),
    };

    let test_bytecode_hash = H256::from([
        0x10, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ]);

    let priority_tx_max_gas_limit = U256::from(72000000);

    let protocol_version = 21;

    let force_deployments_data = vec![];
    let hex_one_32_bytes = u256_to_h256(1.into()).0;
    let action = Action::Add;
    let mailbox_facet_cut = facet_cut(
        mailbox_facet_address,
        &mailbox_facet_contract,
        action.clone(),
        true,
    );
    let admin_facet_cut = facet_cut(admin_facet_address, &admin_facet_contract, action, false);

    let diamond_cut = compile_initial_cut_hash(
        vec![admin_facet_cut, mailbox_facet_cut],
        verifier_params,
        test_bytecode_hash,
        test_bytecode_hash,
        deploy_account_address,
        deploy_account_address,
        priority_tx_max_gas_limit,
        diamond_init_address,
        deploy_account_address,
    );

    let chain_creation_params = ChainCreationParams {
        genesis_upgrade: deploy_account_address,
        genesis_batch_hash: H256::from(hex_one_32_bytes),
        genesis_index_repeated_storage_changes: U256::from(hex_one_32_bytes),
        genesis_batch_commitment: H256::from(hex_one_32_bytes),
        diamond_cut: diamond_cut.clone(),
        force_deployments_data,
    };

    let stm_initialize_data = Token::Tuple(vec![
        Token::Address(deploy_account_address),
        Token::Address(deploy_account_address),
        chain_creation_params.to_token(),
        Token::Uint(U256::from(protocol_version)),
    ]);

    let init_calldata = stm_contract
        .function("initialize")
        .unwrap()
        .encode_input(&[stm_initialize_data])
        .unwrap();

    // Deploy Transaprent Upgradeable Proxy

    let (transparent_proxy_contract_code, _transparent_proxy_contract) = read_transparent_proxy();
    let constructor_data = &[
        Token::Address(stm_address),
        Token::Address(deploy_account_address),
        Token::Bytes(init_calldata),
    ];
    let stm_proxy_address = deploy_and_verify_contract(
        vm,
        0,
        &transparent_proxy_contract_code,
        Some(constructor_data),
        &mut deploy_nonce,
    );

    // Call BH to mint new chain

    let (_bridgehub_contract_bytecode, bridgehub_contract) = read_bridgehub();

    // Set addresses

    let set_addresses_calldata = bridgehub_contract
        .function("setAddresses")
        .unwrap()
        .encode_input(&[
            Token::Address(L2_ASSET_ROUTER_ADDRESS),
            Token::Address(undo_l1_to_l2_alias(deploy_account_address)),
            Token::Address(message_root_address),
        ])
        .unwrap();
    send_l2_tx_and_verify(vm, bridgehub_address, set_addresses_calldata);

    // Add STM

    let add_stm_calldata = bridgehub_contract
        .function("addStateTransitionManager")
        .unwrap()
        .encode_input(&[Token::Address(stm_proxy_address)])
        .unwrap();
    send_l2_tx_and_verify(vm, bridgehub_address, add_stm_calldata);

    // Set Asset Handler Address

    let asset_id = Token::FixedBytes(vec![0, 32]);
    let set_asset_handler_calldata = bridgehub_contract
        .function("setAssetHandlerAddress")
        .unwrap()
        .encode_input(&[asset_id.clone(), Token::Address(stm_proxy_address)])
        .unwrap();
    send_l2_tx_and_verify(vm, bridgehub_address, set_asset_handler_calldata);

    // Deploy New Chain via BH BridgeMint

    // Define the dummy data
    let chain_id: Token = Token::Uint(272.into());

    // Create the tokens for each parameter
    let base_token_token = Token::Address(SHARED_BRIDGE_ETHER_TOKEN_ADDRESS);
    let admin_token = Token::Address(deploy_account_address);
    let protocol_version_token = Token::Uint(U256::from(21));
    let diamond_cut_token = encode(&[diamond_cut.to_token()]);

    // Encode the data into a single bytes array
    let stm_data = encode(&[
        base_token_token,
        admin_token,
        protocol_version_token,
        Token::Bytes(diamond_cut_token.clone()),
    ]);

    // Step 1: Define the data
    let total_batches_executed: U256 = 10u64.into(); // Dummy value
    let total_batches_verified: U256 = 15u64.into(); // Must be >= total_batches_executed
    let total_batches_committed: U256 = 20u64.into(); // Must be >= total_batches_verified

    let l2_system_contracts_upgrade_tx_hash = H256::zero(); // Dummy 0x000...000
    let l2_system_contracts_upgrade_batch_number = 5u64.into(); // Dummy value

    // Generate dummy batch hashes (bytes32) with the correct length
    let num_hashes = total_batches_committed.as_u64() - total_batches_executed.as_u64() + 1;
    let batch_hashes: Vec<H256> = (0..num_hashes).map(|_| H256::zero()).collect(); // Dummy 0x000...000

    // Constructing the priority tree commitment
    let next_leaf_index = 30u64.into(); // Dummy value
    let start_index = 25u64.into(); // Dummy value
    let unprocessed_index = 28u64.into(); // Dummy value

    // Generate dummy sides (bytes32) with arbitrary length
    let sides: Vec<H256> = (0..3).map(|_| H256::zero()).collect(); // Dummy 0x000...000

    // Step 2: Convert the PriorityTreeCommitment to Tokens
    let priority_tree_commitment = Token::Tuple(vec![
        Token::Uint(next_leaf_index),
        Token::Uint(start_index),
        Token::Uint(unprocessed_index),
        Token::Array(
            sides
                .iter()
                .map(|hash| Token::FixedBytes(hash.as_bytes().to_vec()))
                .collect(),
        ),
    ]);

    // Step 3: Convert the HyperchainCommitment to Tokens
    let hyperchain_commitment = Token::Tuple(vec![
        Token::Uint(total_batches_executed),
        Token::Uint(total_batches_verified),
        Token::Uint(total_batches_committed),
        Token::FixedBytes(l2_system_contracts_upgrade_tx_hash.as_bytes().to_vec()),
        Token::Uint(l2_system_contracts_upgrade_batch_number),
        Token::Array(
            batch_hashes
                .iter()
                .map(|hash| Token::FixedBytes(hash.as_bytes().to_vec()))
                .collect(),
        ),
        priority_tree_commitment,
    ]);

    // Step 4: Encode the data
    let chain_mint_data = encode(&[hyperchain_commitment]);

    // Encode the data
    let encoded_bridgehub_mint_data = encode(&[
        chain_id.clone(),
        Token::Bytes(stm_data),
        Token::Bytes(chain_mint_data),
    ]);

    let sender = Token::Address(undo_l1_to_l2_alias(deploy_account_address)); // Replace with actual sender address
    let asset_id = Token::FixedBytes(vec![0, 32]);
    let additional_data = asset_id;

    // Encode the data
    let encoded_data = encode(&[l1_chain_id, sender, additional_data]);

    // Calculate the keccak256 hash of the encoded data
    let asset_info = keccak256(&encoded_data);

    let chain_mint_calldata = bridgehub_contract
        .function("bridgeMint")
        .unwrap()
        .encode_input(&[
            chain_id,
            Token::FixedBytes(asset_info.to_vec()),
            Token::Bytes(encoded_bridgehub_mint_data),
        ])
        .unwrap();
    send_prank_tx_and_verify(
        vm,
        bridgehub_address,
        vec![
            diamond_proxy_contract_code,
            diamond_init_contract_code,
            diamond_contract_code,
        ],
        chain_mint_calldata,
        L2_ASSET_ROUTER_ADDRESS,
    );

    (bridgehub_address, bridgehub_contract)
}

#[test]
fn test_l1_l2_complete_tx_execution_many_small_factory_deps() {
    // In this test, we try to execute a transaction from L1 to L2 via Gateway
    // Here instead of marking code hash via the bootloader means, we will be
    // using L1->Gateway->L2 communication, the same it would likely be done during the priority mode.

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let default_account: Account = vm.rich_accounts[0].clone();

    let (bridgehub_address, bridgehub_contract) =
        prepare_environment_and_deploy_contracts(&mut vm, default_account.address);

    // Collect Data for Test

    let request_params = BridgeHubRequestL2TransactionOnGateway::default();

    // Generate a large number of vectors
    let small_vector_size: usize = 32; // Size of each vector
    let num_vectors = 10_676; // Number of vectors

    let factory_deps: Vec<Vec<u8>> = (0..num_vectors)
        .map(|_| vec![0u8; small_vector_size])
        .collect();

    // Update request_params with the current factory_deps
    let mut modified_request_params = request_params.clone();
    modified_request_params.factory_deps = factory_deps.clone();

    let encoded_data = bridgehub_contract
        .function("forwardTransactionOnGateway")
        .unwrap()
        .encode_input(&modified_request_params.to_tokens())
        .unwrap();

    println!("Factory deps data size: {}", factory_deps.len());
    println!("Encoded data size: {}", encoded_data.len());

    send_prank_tx_and_verify(
        &mut vm,
        bridgehub_address,
        vec![],
        encoded_data,
        SETTLEMENT_LAYER_RELAY_SENDER,
    );
}

#[test]
fn test_l1_l2_complete_tx_execution_few_large_factory_deps() {
    // In this test, we try to execute a transaction from L1 to L2 via Gateway
    // Here instead of marking code hash via the bootloader means, we will be
    // using L1->Gateway->L2 communication, the same it would likely be done during the priority mode.

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let default_account: Account = vm.rich_accounts[0].clone();

    let (bridgehub_address, bridgehub_contract) =
        prepare_environment_and_deploy_contracts(&mut vm, default_account.address);

    // Collect Data for Test

    let request_params = BridgeHubRequestL2TransactionOnGateway::default();

    // Generate a large number of vectors
    let small_vector_size: usize = 45_824; // Size of each vector
    let num_vectors = 32; // Number of vectors

    let factory_deps: Vec<Vec<u8>> = (0..num_vectors)
        .map(|_| vec![0u8; small_vector_size])
        .collect();

    // Update request_params with the current factory_deps
    let mut modified_request_params = request_params.clone();
    modified_request_params.factory_deps = factory_deps.clone();

    let encoded_data = bridgehub_contract
        .function("forwardTransactionOnGateway")
        .unwrap()
        .encode_input(&modified_request_params.to_tokens())
        .unwrap();

    println!("Factory deps data size: {}", factory_deps.len());
    println!("Encoded data size: {}", encoded_data.len());

    send_prank_tx_and_verify(
        &mut vm,
        bridgehub_address,
        vec![],
        encoded_data,
        SETTLEMENT_LAYER_RELAY_SENDER,
    );
}

#[test]
fn test_l1_l2_complete_tx_execution_one_large_factory_dep() {
    // In this test, we try to execute a transaction from L1 to L2 via Gateway
    // Here instead of marking code hash via the bootloader means, we will be
    // using L1->Gateway->L2 communication, the same it would likely be done during the priority mode.

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let default_account: Account = vm.rich_accounts[0].clone();

    let (bridgehub_address, bridgehub_contract) =
        prepare_environment_and_deploy_contracts(&mut vm, default_account.address);

    // Collect Data for Test

    let request_params = BridgeHubRequestL2TransactionOnGateway::default();

    // Generate a large number of vectors
    let small_vector_size: usize = 1_470_560; // Size of each vector
    let num_vectors = 1; // Number of vectors

    let factory_deps: Vec<Vec<u8>> = (0..num_vectors)
        .map(|_| vec![0u8; small_vector_size])
        .collect();

    // Update request_params with the current factory_deps
    let mut modified_request_params = request_params.clone();
    modified_request_params.factory_deps = factory_deps.clone();

    let encoded_data = bridgehub_contract
        .function("forwardTransactionOnGateway")
        .unwrap()
        .encode_input(&modified_request_params.to_tokens())
        .unwrap();

    println!("Factory deps data size: {}", factory_deps.len());
    println!("Encoded data size: {}", encoded_data.len());

    send_prank_tx_and_verify(
        &mut vm,
        bridgehub_address,
        vec![],
        encoded_data,
        SETTLEMENT_LAYER_RELAY_SENDER,
    );
}

#[test]
fn test_l1_l2_complete_tx_execution_modified_l2_canonical_tx_calldata() {
    // In this test, we try to execute a transaction from L1 to L2 via Gateway
    // Here instead of marking code hash via the bootloader means, we will be
    // using L1->Gateway->L2 communication, the same it would likely be done during the priority mode.

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let default_account: Account = vm.rich_accounts[0].clone();

    let (bridgehub_address, bridgehub_contract) =
        prepare_environment_and_deploy_contracts(&mut vm, default_account.address);

    // Collect Data for Test

    let request_params = BridgeHubRequestL2TransactionOnGateway::default();

    // Generate a large number of vectors
    let small_vector_size: usize = 1_470_560; // Size of each vector
    let num_vectors = 1; // Number of vectors

    let factory_deps: Vec<Vec<u8>> = (0..num_vectors)
        .map(|_| vec![0u8; small_vector_size])
        .collect();

    let call_data_size: usize = 3_879_168; // Size of calldata of L2 tx
    let call_data: Vec<u8> = vec![1u8; call_data_size];

    // Update request_params with the current factory_deps
    let mut modified_request_params = request_params.clone();
    modified_request_params.factory_deps = factory_deps.clone();
    modified_request_params.transaction.data = call_data;

    let encoded_data = bridgehub_contract
        .function("forwardTransactionOnGateway")
        .unwrap()
        .encode_input(&modified_request_params.to_tokens())
        .unwrap();

    // Total tx size increase > 2x due to calldata increase
    println!("Factory deps data size: {}", factory_deps.len());
    println!("Encoded data size: {}", encoded_data.len());

    send_prank_tx_and_verify(
        &mut vm,
        bridgehub_address,
        vec![],
        encoded_data,
        SETTLEMENT_LAYER_RELAY_SENDER,
    );
}

#[test]
fn test_l1_l2_complete_tx_execution_modified_l2_canonical_tx_calldata_factory_deps() {
    // In this test, we try to execute a transaction from L1 to L2 via Gateway
    // Here instead of marking code hash via the bootloader means, we will be
    // using L1->Gateway->L2 communication, the same it would likely be done during the priority mode.

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    let default_account: Account = vm.rich_accounts[0].clone();

    let (bridgehub_address, bridgehub_contract) =
        prepare_environment_and_deploy_contracts(&mut vm, default_account.address);

    // Collect Data for Test

    let request_params = BridgeHubRequestL2TransactionOnGateway::default();

    // Generate a large number of vectors
    let small_vector_size: usize = 1_470_560; // Size of each vector
    let num_vectors = 1; // Number of vectors

    let factory_deps: Vec<Vec<u8>> = (0..num_vectors)
        .map(|_| vec![0u8; small_vector_size])
        .collect();

    let call_data_size: usize = 2_879_168; // Size of calldata of L2 tx
    let call_data: Vec<u8> = vec![1u8; call_data_size]; // 5350756

    let tx_factory_deps_size: usize = 32_209; // Size of factory deps of L2 tx
    let tx_factory_deps = vec![U256::from(1); tx_factory_deps_size]; // Total size 5381476, out of which outer factory deps are 1470592 (tx is ~3910884)

    // Update request_params with the current factory_deps
    let mut modified_request_params = request_params.clone();
    modified_request_params.factory_deps = factory_deps.clone();
    modified_request_params.transaction.data = call_data;
    modified_request_params.transaction.factory_deps = tx_factory_deps;

    let encoded_data = bridgehub_contract
        .function("forwardTransactionOnGateway")
        .unwrap()
        .encode_input(&modified_request_params.to_tokens())
        .unwrap();

    // Total tx size only changes by a few %, as more data is assigned to one param vs another
    println!("Factory deps data size: {}", factory_deps.len());
    println!("Encoded data size: {}", encoded_data.len());

    send_prank_tx_and_verify(
        &mut vm,
        bridgehub_address,
        vec![],
        encoded_data,
        SETTLEMENT_LAYER_RELAY_SENDER,
    );
}
