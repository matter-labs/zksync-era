use ethabi::{encode, Bytes, Contract, Function, Hash, Token};
use tiny_keccak::{Hasher, Keccak}; // Import the Hasher trait
use zksync_basic_types::{Address, H160, H256};
use zksync_contracts::{l1_messenger_contract, mailbox_contract};
use zksync_system_constants::{
    BOOTLOADER_ADDRESS, L1_MESSENGER_ADDRESS, L2_ASSET_ROUTER_ADDRESS, L2_BRIDGEHUB_ADDRESS,
    L2_MESSAGE_ROOT_ADDRESS, SHARED_BRIDGE_ETHER_TOKEN_ADDRESS,
};
use zksync_test_account::Account;
use zksync_types::{
    fee::Fee,
    get_code_key, get_known_code_key,
    l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
    mailbox::BridgeHubRequestL2TransactionOnGateway,
    Execute, ExecuteTransactionCommon, K256PrivateKey, U256,
};
use zksync_utils::u256_to_h256;

use crate::{
    interface::{ExecutionResult, TxExecutionMode, VmExecutionMode, VmInterface},
    utils::{bytecode::encode_call, StorageWritesDeduplicator},
    vm_latest::{
        tests::{
            tester::{TxType, VmTesterBuilder},
            utils::{
                read_bridgehub, read_diamond_init, read_mailbox_facet, read_message_root, read_stm,
                read_transparent_proxy, verify_required_storage, BASE_SYSTEM_CONTRACTS,
            },
        },
        types::internals::TransactionData,
        HistoryEnabled,
    },
};

#[derive(Debug, Clone)]
pub struct VerifierParams {
    recursion_node_level_vk_hash: H256,
    recursion_leaf_level_vk_hash: H256,
    recursion_circuits_set_vks_hash: H256,
}

#[derive(Debug, Clone)]
pub enum PubdataPricingMode {
    Rollup,
    Validium,
}

#[derive(Debug, Clone)]
pub struct FeeParams {
    pubdata_pricing_mode: PubdataPricingMode,
    batch_overhead_l1_gas: U256,
    max_pubdata_per_batch: U256,
    priority_tx_max_pubdata: U256,
    max_l2_gas_per_batch: U256,
    minimal_l2_gas_price: U256,
}

#[derive(Debug, Clone)]
pub struct FacetCut {
    pub facet: Address,
    pub action: Action,
    pub is_freezable: bool,
    pub selectors: Vec<[u8; 4]>, // Assuming selectors are 4-byte arrays as in Ethereum function selectors
}

impl FacetCut {
    pub fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::Address(self.facet),
            Token::Uint(self.action.to_uint()), // Convert Action to U256 or Token::Uint as needed
            Token::Bool(self.is_freezable),
            Token::Array(
                self.selectors
                    .iter()
                    .map(|selector| Token::FixedBytes(selector.to_vec()))
                    .collect(),
            ),
        ])
    }
}

#[derive(Debug, Clone)]
pub struct DiamondCut {
    facet_cuts: Vec<FacetCut>,
    init_address: Address,
    init_calldata: Bytes,
}

impl DiamondCut {
    pub fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::Array(
                self.facet_cuts
                    .iter()
                    .map(|facet_cut| facet_cut.to_token())
                    .collect(),
            ),
            Token::Address(self.init_address),
            Token::Bytes(self.init_calldata.clone()),
        ])
    }
}

#[derive(Debug, Clone)]
struct ChainCreationParams {
    genesis_upgrade: Address, // Assuming this is an Ethereum address
    genesis_batch_hash: H256, // 32-byte hash
    genesis_index_repeated_storage_changes: U256, // Large unsigned integer
    genesis_batch_commitment: H256, // 32-byte hash
    diamond_cut: DiamondCut,  // List of facet cuts
    force_deployments_data: Vec<u8>, // Assuming this is binary data
}

impl ChainCreationParams {
    pub fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::Address(self.genesis_upgrade),
            Token::FixedBytes(self.genesis_batch_hash.as_bytes().to_vec()),
            Token::Uint(self.genesis_index_repeated_storage_changes),
            Token::FixedBytes(self.genesis_batch_commitment.as_bytes().to_vec()),
            self.diamond_cut.to_token(), // Assuming `DiamondCut` has a `to_token` method
            Token::Bytes(self.force_deployments_data.clone()),
        ])
    }
}

#[derive(Debug, Clone)]
pub struct DiamondInitData {
    pub chain_id: U256,
    pub bridgehub: Address,
    pub state_transition_manager: Address,
    pub protocol_version: U256,
    pub admin: Address,
    pub validator_timelock: Address,
    pub base_token: Address,
    pub base_token_bridge: Address,
    pub stored_batch_zero: H256,
    pub verifier: Address,
    pub verifier_params: VerifierParams,
    pub l2_bootloader_bytecode_hash: H256,
    pub l2_default_account_bytecode_hash: H256,
    pub priority_tx_max_gas_limit: U256,
    pub fee_params: FeeParams,
    pub blob_versioned_hash_retriever: Address,
}

impl DiamondInitData {
    pub fn to_token(&self) -> Token {
        Token::Tuple(vec![
            Token::Uint(self.chain_id),
            Token::Address(self.bridgehub),
            Token::Address(self.state_transition_manager),
            Token::Uint(self.protocol_version),
            Token::Address(self.admin),
            Token::Address(self.validator_timelock),
            Token::Address(self.base_token),
            Token::Address(self.base_token_bridge),
            Token::FixedBytes(self.stored_batch_zero.as_bytes().to_vec()),
            Token::Address(self.verifier),
            Token::Tuple(vec![
                Token::FixedBytes(
                    self.verifier_params
                        .recursion_node_level_vk_hash
                        .as_bytes()
                        .to_vec(),
                ),
                Token::FixedBytes(
                    self.verifier_params
                        .recursion_leaf_level_vk_hash
                        .as_bytes()
                        .to_vec(),
                ),
                Token::FixedBytes(
                    self.verifier_params
                        .recursion_circuits_set_vks_hash
                        .as_bytes()
                        .to_vec(),
                ),
            ]),
            Token::FixedBytes(self.l2_bootloader_bytecode_hash.as_bytes().to_vec()),
            Token::FixedBytes(self.l2_default_account_bytecode_hash.as_bytes().to_vec()),
            Token::Uint(self.priority_tx_max_gas_limit),
            Token::Tuple(vec![
                Token::Uint(self.fee_params.batch_overhead_l1_gas),
                Token::Uint(self.fee_params.max_pubdata_per_batch),
                Token::Uint(self.fee_params.priority_tx_max_pubdata),
                Token::Uint(self.fee_params.max_l2_gas_per_batch),
                Token::Uint(self.fee_params.minimal_l2_gas_price),
            ]),
            Token::Address(self.blob_versioned_hash_retriever),
        ])
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Add,
    Replace,
    Remove,
}

// Assuming `Action` enum has a method `to_uint` to convert it to `Token::Uint`
impl Action {
    pub fn to_uint(&self) -> U256 {
        match self {
            Action::Add => U256::from(0),
            Action::Replace => U256::from(1),
            Action::Remove => U256::from(2),
        }
    }
}

// Function to get the selectors
fn get_all_selectors(contract: &Contract) -> Vec<[u8; 4]> {
    contract
        .functions
        .iter()
        .filter_map(|(signature, functions)| {
            if signature != "getName()" {
                // We assume the function has only one variant in this context
                // We take the first variant in the Vec<Function>
                Some(functions[0].short_signature())
            } else {
                None
            }
        })
        .collect()
}

fn facet_cut(
    address: Address,
    contract: &Contract,
    action: Action,
    is_freezable: bool,
) -> FacetCut {
    FacetCut {
        facet: address,
        selectors: get_all_selectors(contract),
        action,
        is_freezable,
    }
}

fn compile_initial_cut_hash(
    facet_cuts: Vec<FacetCut>,
    verifier_params: VerifierParams,
    l2_bootloader_bytecode_hash: H256,
    l2_default_account_bytecode_hash: H256,
    verifier: Address,
    blob_versioned_hash_retriever: Address,
    priority_tx_max_gas_limit: U256,
    diamond_init: Address,
    diamond_init_contract: Contract,
    admin: Address,
) -> DiamondCut {
    // Define the fee parameters
    let fee_params = FeeParams {
        pubdata_pricing_mode: PubdataPricingMode::Rollup,
        batch_overhead_l1_gas: U256::from(1000000), // Replace with actual value
        max_pubdata_per_batch: U256::from(1000000), // Replace with actual value
        priority_tx_max_pubdata: U256::from(1000000), // Replace with actual value
        max_l2_gas_per_batch: U256::from(1000000),  // Replace with actual value
        minimal_l2_gas_price: U256::from(1000),     // Replace with actual value
    };

    // Initialize the diamond with the required data
    let diamond_init_data = DiamondInitData {
        chain_id: U256::from(1),
        bridgehub: L2_BRIDGEHUB_ADDRESS,
        state_transition_manager: "0x0000000000000000000000000000000000002234"
            .parse()
            .unwrap(),
        protocol_version: U256::from(21),
        admin: admin.into(),
        validator_timelock: "0x0000000000000000000000000000000000004234"
            .parse()
            .unwrap(),
        base_token: "0x0000000000000000000000000000000000000001"
            .parse()
            .unwrap(),
        base_token_bridge: "0x0000000000000000000000000000000000004234"
            .parse()
            .unwrap(),
        stored_batch_zero: H256::from([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x12, 0x34,
        ]),
        verifier,
        verifier_params,
        l2_bootloader_bytecode_hash,
        l2_default_account_bytecode_hash,
        priority_tx_max_gas_limit,
        fee_params,
        blob_versioned_hash_retriever,
    };

    let encoded_data;
    let init_calldata_result = diamond_init_contract
        .function("initialize")
        .unwrap()
        .encode_input(&[diamond_init_data.to_token()]);

    match init_calldata_result {
        Ok(init_calldata) => {
            // Proceed with using `init_calldata`
            encoded_data = init_calldata;

            // Calculate the start index
            let start_index = 2 + (4 + 9 * 32) * 2;

            let hex_calldata = hex::encode(encoded_data);

            // Slice the string from the calculated index
            let sliced_calldata = &hex_calldata[start_index..];

            // Prepend "0x" to the sliced string
            let result = format!("0x{}", sliced_calldata);

            // Return the DiamondCut struct
            DiamondCut {
                facet_cuts,
                init_address: diamond_init,
                init_calldata: result.into(),
            }
        }
        Err(e) => {
            // Handle the error
            println!("Error encoding calldata: {:?}", e);
            unreachable!("asda");
        }
    }
}

#[test]
fn test_l1_l2_complete_tx_execution() {
    // In this test, we try to execute a transaction from L1 to L2 via Gateway
    // Here instead of marking code hash via the bootloader means, we will be
    // using L1->Gateway->L2 communication, the same it would likely be done during the priority mode.

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

    // TODO:
    // 1. Deploy STM
    // 2. Deploy Mailbox Facet
    // 3. Deploy Diamond Init
    // 4. Prepare STM initialize Data
    // 5. Deploy STM proxy
    // 5. Deploy new Chain via call from Bridgehub
    // 6. Test tx as before
    let basic_initial_writes = 5;

    let mut vm = VmTesterBuilder::new(HistoryEnabled)
        .with_empty_in_memory_storage()
        .with_base_system_smart_contracts(BASE_SYSTEM_CONTRACTS.clone())
        .with_execution_mode(TxExecutionMode::VerifyExecute)
        .with_random_rich_accounts(1)
        .build();

    // Deploy STM

    let account: &mut Account = &mut vm.rich_accounts[0];
    let (stm_contract_code, stm_contract_abi) = read_stm();
    // Set the constructor data to L2 bridgehub address and max number of hyperchains to 100
    let max_number_of_hyperchains = U256::from(100);
    let constructor_data = &[
        Token::Address(L2_BRIDGEHUB_ADDRESS),
        Token::Uint(max_number_of_hyperchains),
    ];
    let stm_deploy_tx = account.get_deploy_tx(
        &stm_contract_code,
        Some(constructor_data),
        TxType::L1 { serial_id: 1 },
    );
    let stm_address = stm_deploy_tx.address.clone();

    vm.vm.push_transaction(stm_deploy_tx.tx.clone());

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    // The code hash of the deployed contract should be marked as republished.
    let known_codes_key = get_known_code_key(&stm_deploy_tx.bytecode_hash);

    // The contract should be deployed successfully.
    let account_code_key = get_code_key(&stm_deploy_tx.address);

    let expected_slots = vec![
        (u256_to_h256(U256::from(1u32)), known_codes_key),
        (stm_deploy_tx.bytecode_hash, account_code_key),
    ];
    assert!(!res.result.is_failed());

    verify_required_storage(&vm.vm.state, expected_slots);

    // Deploy Mailbox Facet

    let (mailbox_facet_contract_code, mailbox_facet_abi) = read_mailbox_facet();
    let era_chain_id = U256::from(1);
    let constructor_data = &[Token::Uint(era_chain_id)];
    let mailbox_facet_deploy_tx = account.get_deploy_tx(
        &mailbox_facet_contract_code,
        Some(constructor_data),
        TxType::L1 { serial_id: 1 },
    );
    let mailbox_facet_address = mailbox_facet_deploy_tx.address.clone();

    vm.vm.push_transaction(mailbox_facet_deploy_tx.tx.clone());

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    // // The code hash of the deployed contract should be marked as republished.
    // let known_codes_key = get_known_code_key(&deploy_tx_mailbox_facet.bytecode_hash);

    // // The contract should be deployed successfully.
    // let account_code_key = get_code_key(&deploy_tx_mailbox_facet.address);

    // let expected_slots = vec![
    //     (u256_to_h256(U256::from(1u32)), known_codes_key),
    //     (deploy_tx_mailbox_facet.bytecode_hash, account_code_key),
    // ];

    // match res.result {
    //     ExecutionResult::Success { output } => {
    //         println!("Transaction was successful. Output: {:?}", output);
    //     }
    //     ExecutionResult::Revert { output } => {
    //         eprintln!("Transaction reverted. Reason: {:?}", output);
    //         panic!("Transaction wasn't successful: {:?}", output);
    //     }
    //     ExecutionResult::Halt { reason } => {
    //         eprintln!("Transaction halted. Reason: {:?}", reason);
    //         panic!("Transaction wasn't successful: {:?}", reason);
    //     }
    // }

    assert!(!res.result.is_failed());

    // verify_required_storage(&vm.vm.state, expected_slots);

    // Deploy Diamond Init

    let (diamond_init_contract_code, diamond_init_contract) = read_diamond_init();
    let diamond_init_deploy_tx = account.get_deploy_tx(
        &diamond_init_contract_code,
        None,
        TxType::L1 { serial_id: 1 },
    );
    let diamond_init_address = diamond_init_deploy_tx.address.clone();

    vm.vm.push_transaction(diamond_init_deploy_tx.tx.clone());

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    assert!(!res.result.is_failed());

    // Initialize STM

    let action = Action::Add;
    let mailbox_facet_cut = facet_cut(mailbox_facet_address, &mailbox_facet_abi, action, true);

    println!("{:?}", mailbox_facet_cut);

    let genesis_batch_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";
    let genesis_rollup_leaf_index = "1";
    let genesis_batch_commitment =
        "0x0000000000000000000000000000000000000000000000000000000000000001";

    let verifier_params = VerifierParams {
        recursion_node_level_vk_hash: H256::from([0u8; 32]),
        recursion_leaf_level_vk_hash: H256::from([0u8; 32]),
        recursion_circuits_set_vks_hash: H256::from([0u8; 32]),
    };

    let l2_bootloader_bytecode_hash = H256::from([
        0x10, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ]);
    let l2_default_account_bytecode_hash = H256::from([
        0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ]);

    let priority_tx_max_gas_limit = U256::from(72000000);

    let diamond_cut = compile_initial_cut_hash(
        vec![mailbox_facet_cut],
        verifier_params,
        l2_bootloader_bytecode_hash,
        l2_default_account_bytecode_hash,
        account.address,
        account.address,
        priority_tx_max_gas_limit,
        diamond_init_address,
        diamond_init_contract,
        account.address,
    );

    let protocol_version = 21;

    // Should affect in this scenario
    let force_deployments_data = vec![];
    let hex_one_32_bytes = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x01,
    ];
    let genesis_batch_commitment = H256::from(hex_one_32_bytes);

    let chain_creation_params = ChainCreationParams {
        genesis_upgrade: account.address,
        genesis_batch_hash: H256::from(hex_one_32_bytes),
        genesis_index_repeated_storage_changes: U256::from(genesis_rollup_leaf_index),
        genesis_batch_commitment,
        diamond_cut: diamond_cut.clone(),
        force_deployments_data,
    };

    let stm_initialize_data = Token::Tuple(vec![
        Token::Address(account.address),
        Token::Address(account.address),
        chain_creation_params.to_token(),
        Token::Uint(U256::from(protocol_version)),
    ]);

    println!(
        "STM Initialize Data (Simple) Token: {:?}",
        stm_initialize_data
    );

    let init_calldata = stm_contract_abi
        .function("initialize")
        .unwrap()
        .encode_input(&[stm_initialize_data])
        .unwrap();

    // let tx = account.get_l2_tx_for_execute(
    //     Execute {
    //         contract_address: stm_address,
    //         calldata: init_calldata,
    //         value: U256::zero(),
    //         factory_deps: vec![],
    //     },
    //     None,
    // );
    // vm.vm.push_transaction(tx);

    // let result = vm.vm.execute(VmExecutionMode::OneTx);
    // // Check if the transaction failed
    // match result.result {
    //     ExecutionResult::Success { output } => {
    //         println!("Transaction was successful. Output: {:?}", output);
    //     }
    //     ExecutionResult::Revert { output } => {
    //         println!("Transaction reverted. Reason: {:?}", output);
    //         panic!("Transaction wasn't successful: {:?}", output);
    //     }
    //     ExecutionResult::Halt { reason } => {
    //         println!("Transaction halted. Reason: {:?}", reason);
    //         panic!("Transaction wasn't successful: {:?}", reason);
    //     }
    // }

    // Deploy Transaprent Upgradeable Proxy

    let message_root_contract_bytecode = read_message_root();
    let constructor_data = &[Token::Address(L2_BRIDGEHUB_ADDRESS)];
    let message_root_deploy_tx = account.get_deploy_tx(
        &message_root_contract_bytecode,
        Some(constructor_data),
        TxType::L1 { serial_id: 1 },
    );
    let message_root_address = message_root_deploy_tx.address.clone();

    vm.vm.push_transaction(message_root_deploy_tx.tx.clone());

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    assert!(!res.result.is_failed());

    // Deploy Transaprent Upgradeable Proxy

    let (transparent_proxy_contract_bytecode, transparent_proxy_contract) =
        read_transparent_proxy();
    let constructor_data = &[
        Token::Address(stm_address),
        Token::Address(account.address),
        Token::Bytes(init_calldata),
    ];
    let stm_proxy_deploy_tx = account.get_deploy_tx(
        &transparent_proxy_contract_bytecode,
        Some(constructor_data),
        TxType::L1 { serial_id: 1 },
    );
    let stm_proxy_address = stm_proxy_deploy_tx.address.clone();

    vm.vm.push_transaction(stm_proxy_deploy_tx.tx.clone());

    let res = vm.vm.execute(VmExecutionMode::OneTx);

    // Check if the transaction failed
    match res.result {
        ExecutionResult::Success { output } => {
            println!("Transaction was successful. Output: {:?}", output);
        }
        ExecutionResult::Revert { output } => {
            println!("Transaction reverted. Reason: {:?}", output);
            panic!("Transaction wasn't successful: {:?}", output);
        }
        ExecutionResult::Halt { reason } => {
            println!("Transaction halted. Reason: {:?}", reason);
            panic!("Transaction wasn't successful: {:?}", reason);
        }
    }

    // assert!(!res.result.is_failed());

    // Call BH to mint new chain

    let (bridgehub_contract_bytecode, bridgehub_contract) = read_bridgehub();

    // Initialize
    let initialize_calldata = bridgehub_contract
        .function("initialize")
        .unwrap()
        .encode_input(&[Token::Address(account.address)])
        .unwrap();

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_BRIDGEHUB_ADDRESS,
            calldata: initialize_calldata,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let result = vm.vm.execute(VmExecutionMode::OneTx);

    // Check if the transaction failed
    match result.result {
        ExecutionResult::Success { output } => {
            println!("Transaction was successful. Output: {:?}", output);
        }
        ExecutionResult::Revert { output } => {
            println!("Transaction reverted. Reason: {:?}", output);
            panic!("Transaction wasn't successful: {:?}", output);
        }
        ExecutionResult::Halt { reason } => {
            println!("Transaction halted. Reason: {:?}", reason);
            panic!("Transaction wasn't successful: {:?}", reason);
        }
    }

    // Set addresses

    let set_addresses_calldata = bridgehub_contract
        .function("setAddresses")
        .unwrap()
        .encode_input(&[
            Token::Address(L2_ASSET_ROUTER_ADDRESS),
            Token::Address(account.address),
            Token::Address(message_root_address),
        ])
        .unwrap();

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_BRIDGEHUB_ADDRESS,
            calldata: set_addresses_calldata,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let result = vm.vm.execute(VmExecutionMode::OneTx);

    // Check if the transaction failed
    match result.result {
        ExecutionResult::Success { output } => {
            println!("Transaction was successful. Output: {:?}", output);
        }
        ExecutionResult::Revert { output } => {
            println!("Transaction reverted. Reason: {:?}", output);
            panic!("Transaction wasn't successful: {:?}", output);
        }
        ExecutionResult::Halt { reason } => {
            println!("Transaction halted. Reason: {:?}", reason);
            panic!("Transaction wasn't successful: {:?}", reason);
        }
    }

    // Set Asset Handler Address

    let asset_id = Token::FixedBytes(vec![0, 32]);
    let set_asset_handler_calldata = bridgehub_contract
        .function("setAssetHandlerAddress")
        .unwrap()
        .encode_input(&[asset_id.clone(), Token::Address(stm_proxy_address)])
        .unwrap();

    let tx = account.get_l2_tx_for_execute(
        Execute {
            contract_address: L2_BRIDGEHUB_ADDRESS,
            calldata: set_asset_handler_calldata,
            value: U256::zero(),
            factory_deps: vec![],
        },
        None,
    );
    vm.vm.push_transaction(tx);

    let result = vm.vm.execute(VmExecutionMode::OneTx);

    // Check if the transaction failed
    match result.result {
        ExecutionResult::Success { output } => {
            println!("Transaction was successful. Output: {:?}", output);
        }
        ExecutionResult::Revert { output } => {
            println!("Transaction reverted. Reason: {:?}", output);
            panic!("Transaction wasn't successful: {:?}", output);
        }
        ExecutionResult::Halt { reason } => {
            println!("Transaction halted. Reason: {:?}", reason);
            panic!("Transaction wasn't successful: {:?}", reason);
        }
    }

    // Define the dummy data
    let chain_id: Token = Token::Uint(256.into());

    // Create the tokens for each parameter
    let base_token_token = Token::Address(SHARED_BRIDGE_ETHER_TOKEN_ADDRESS);
    let admin_token = Token::Address(account.address);
    let protocol_version_token = Token::Uint(U256::from(21));
    let diamond_cut_token = encode(&[diamond_cut.to_token()]);

    // Encode the data into a single bytes array
    let stm_data = encode(&[
        base_token_token,
        admin_token,
        protocol_version_token,
        Token::Bytes(diamond_cut_token),
    ]);
    println!("{:?}", stm_data);

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

    // Define the L1_CHAIN_ID, sender, and _additionalData
    let l1_chain_id = Token::Uint(0.into()); // Chain ID hasn't been set
    const OFFSET: H160 = H160([
        0x11, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x11, 0x11,
    ]);

    fn sub_h160(lhs: H160, rhs: H160) -> H160 {
        let mut result = [0u8; 20];
        let mut borrow = 0u8;

        for (i, (a, b)) in lhs.0.iter().zip(rhs.0.iter()).enumerate().rev() {
            let (sub, overflow) = a.overflowing_sub(*b);
            let (sub_with_borrow, borrow_overflow) = sub.overflowing_sub(borrow);
            result[i] = sub_with_borrow;
            borrow = (overflow || borrow_overflow) as u8;
        }

        H160(result)
    }

    fn undo_l1_to_l2_alias(l2_address: H160) -> H160 {
        sub_h160(l2_address, OFFSET)
    }

    let sender = Token::Address(undo_l1_to_l2_alias(account.address)); // Replace with actual sender address
    let additional_data = asset_id;

    // Encode the data
    let encoded_data = encode(&[l1_chain_id, sender, additional_data]);

    // Calculate the keccak256 hash of the encoded data
    let mut hasher = Keccak::v256();
    let mut output = [0u8; 32];
    hasher.update(&encoded_data);
    hasher.finalize(&mut output);

    // Print the keccak256 hash
    println!("keccak256 hash: 0x{}", hex::encode(output));

    let asset_info = output;

    let chain_mint_calldata = bridgehub_contract
        .function("bridgeMint")
        .unwrap()
        .encode_input(&[
            chain_id,
            Token::FixedBytes(asset_info.to_vec()),
            Token::Bytes(encoded_bridgehub_mint_data),
        ])
        .unwrap();

    let tx = account.get_l1_prank_tx(
        Execute {
            contract_address: L2_BRIDGEHUB_ADDRESS,
            value: U256::zero(),
            factory_deps: vec![],
            calldata: chain_mint_calldata,
        },
        0,
        L2_ASSET_ROUTER_ADDRESS,
    );

    vm.vm.push_transaction(tx);

    let result = vm.vm.execute(VmExecutionMode::OneTx);

    // Check if the transaction failed
    match result.result {
        ExecutionResult::Success { output } => {
            println!("Transaction was successful. Output: {:?}", output);
        }
        ExecutionResult::Revert { output } => {
            println!("Transaction reverted. Reason: {:?}", output);
            panic!("Transaction wasn't successful: {:?}", output);
        }
        ExecutionResult::Halt { reason } => {
            println!("Transaction halted. Reason: {:?}", reason);
            panic!("Transaction wasn't successful: {:?}", reason);
        }
    }
    // assert!(!result.result.is_failed());

    // // Then, we call the mailbox to simulate BH to ZK chain mailbox call
    // let mailbox_contract = mailbox_contract();
    // let request_params = BridgeHubRequestL2TransactionOnGateway::default();

    // // For 1 bytes array passed the largest tested size that doesn't fail is 9_632_000
    // let vector_size: usize = 9_632_000; // Size of each vector
    // let num_vectors = 1; // Number of vectors

    // let factory_deps: Vec<Vec<u8>> = (0..num_vectors).map(|_| vec![0u8; vector_size]).collect();

    // // Update request_params with the current factory_deps
    // let mut modified_request_params = request_params.clone();
    // modified_request_params.factory_deps = factory_deps.clone();

    // let encoded_data = mailbox_contract
    //     .function("bridgehubRequestL2TransactionOnGateway")
    //     .unwrap()
    //     .encode_input(&modified_request_params.to_tokens())
    //     .unwrap();

    // println!("Factory deps first byte array size: {}", factory_deps[0].len());
    // println!("Encoded data size: {}", encoded_data.len());

    // // Creating a Fee instance with the specified gas limit
    // let fee = Fee {
    //     gas_limit: U256::from(72000000),
    //     max_fee_per_gas: U256::from(1000000000),
    //     max_priority_fee_per_gas: U256::from(1000000000),
    //     gas_per_pubdata_limit: U256::from(1000000000),
    // };

    // let tx = account.get_l2_tx_for_execute(
    //     Execute {
    //         contract_address: mailbox_address,
    //         calldata: encoded_data,
    //         value: U256::zero(),
    //         factory_deps: vec![],
    //     },
    //     Some(fee),
    // );
    // vm.vm.push_transaction(tx);
    // let result = vm.vm.execute(VmExecutionMode::OneTx);
    // // Check if the transaction failed
    // match result.result {
    //     ExecutionResult::Success { output } => {
    //         println!("Transaction was successful. Output: {:?}", output);
    //     }
    //     ExecutionResult::Revert { output } => {
    //         println!("Transaction reverted. Reason: {:?}", output);
    //         panic!("Transaction wasn't successful: {:?}", output);
    //     }
    //     ExecutionResult::Halt { reason } => {
    //         println!("Transaction halted. Reason: {:?}", reason);
    //         panic!("Transaction wasn't successful: {:?}", reason);
    //     }
    // }

    // println!("Transaction failed: {:?}", result.result.error_message());

    // assert!(!result.result.is_failed(), "Transaction wasn't successful");
}
