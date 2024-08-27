use zksync_basic_types::{Address, H256};
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;

use crate::{
    ethabi::{encode, Bytes, Contract, Token},
    U256,
};

#[derive(Debug, Clone)]
pub struct VerifierParams {
    pub recursion_node_level_vk_hash: H256,
    pub recursion_leaf_level_vk_hash: H256,
    pub recursion_circuits_set_vks_hash: H256,
}

#[derive(Debug, Clone)]
pub enum PubdataPricingMode {
    Rollup,
    Validium,
}

#[derive(Debug, Clone)]
pub struct FeeParams {
    pub pubdata_pricing_mode: PubdataPricingMode,
    pub batch_overhead_l1_gas: U256,
    pub max_pubdata_per_batch: U256,
    pub priority_tx_max_pubdata: U256,
    pub max_l2_gas_per_batch: U256,
    pub minimal_l2_gas_price: U256,
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
    pub facet_cuts: Vec<FacetCut>,
    pub init_address: Address,
    pub init_calldata: Bytes,
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
pub struct ChainCreationParams {
    pub genesis_upgrade: Address, // Assuming this is an Ethereum address
    pub genesis_batch_hash: H256, // 32-byte hash
    pub genesis_index_repeated_storage_changes: U256, // Large unsigned integer
    pub genesis_batch_commitment: H256, // 32-byte hash
    pub diamond_cut: DiamondCut,  // List of facet cuts
    pub force_deployments_data: Vec<u8>, // Assuming this is binary data
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
                Token::Uint(U256::from(
                    self.fee_params.pubdata_pricing_mode.clone() as u64
                )),
                Token::Uint(self.fee_params.batch_overhead_l1_gas),
                Token::Uint(self.fee_params.max_pubdata_per_batch),
                Token::Uint(self.fee_params.priority_tx_max_pubdata),
                Token::Uint(self.fee_params.max_l2_gas_per_batch),
                Token::Uint(self.fee_params.minimal_l2_gas_price),
            ]),
            Token::Address(self.blob_versioned_hash_retriever),
        ])
    }

    pub fn to_token_sliced(&self) -> Token {
        Token::Tuple(vec![
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
                Token::Uint(U256::from(
                    self.fee_params.pubdata_pricing_mode.clone() as u64
                )),
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
        .filter_map(|(_signature, functions)| {
            if functions[0].name != "getName" {
                // We assume the function has only one variant in this context
                // We take the first variant in the Vec<Function>
                Some(functions[0].short_signature())
            } else {
                None
            }
        })
        .collect()
}

pub fn facet_cut(
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

pub fn compile_initial_cut_hash(
    facet_cuts: Vec<FacetCut>,
    verifier_params: VerifierParams,
    l2_bootloader_bytecode_hash: H256,
    l2_default_account_bytecode_hash: H256,
    verifier: Address,
    blob_versioned_hash_retriever: Address,
    priority_tx_max_gas_limit: U256,
    diamond_init: Address,
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

    // Return the DiamondCut struct
    DiamondCut {
        facet_cuts,
        init_address: diamond_init,
        init_calldata: encode(&[diamond_init_data.to_token_sliced()]),
    }
}
