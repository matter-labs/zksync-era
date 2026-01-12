use ethers::types::Address;
use rand::Rng;
use serde::{Deserialize, Serialize};
use zkstack_cli_types::L1BatchCommitmentMode;
use zksync_basic_types::{L2ChainId, H256};

use crate::{forge_interface::Create2Addresses, traits::FileConfigTrait, ChainConfig};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RegisterChainL1Config {
    chain: ChainL1Config,
    owner_address: Address,
    contracts: Create2Addresses,
    initialize_legacy_bridge: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChainL1Config {
    pub chain_chain_id: L2ChainId,
    pub base_token_addr: Address,
    pub bridgehub_create_new_chain_salt: u64,
    pub validium_mode: bool,
    pub validator_sender_operator_eth: Address,
    pub validator_sender_operator_blobs_eth: Address,
    /// Additional validators that can be used for prove & execute (when these are handled by different entities).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validator_sender_operator_prove: Option<Address>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validator_sender_operator_execute: Option<Address>,
    pub base_token_gas_price_multiplier_nominator: u64,
    pub base_token_gas_price_multiplier_denominator: u64,
    pub governance_security_council_address: Address,
    pub governance_min_delay: u64,
    pub allow_evm_emulator: bool,
}

impl FileConfigTrait for RegisterChainL1Config {}

impl RegisterChainL1Config {
    pub fn new(chain_config: &ChainConfig, create2_factory_addr: Address) -> anyhow::Result<Self> {
        let initialize_legacy_bridge = chain_config.legacy_bridge.unwrap_or_default();
        let wallets_config = chain_config.get_wallets_config()?;

        Ok(Self {
            chain: ChainL1Config {
                chain_chain_id: chain_config.chain_id,
                base_token_gas_price_multiplier_nominator: chain_config.base_token.nominator,
                base_token_gas_price_multiplier_denominator: chain_config.base_token.denominator,
                base_token_addr: chain_config.base_token.address,
                // TODO specify
                governance_security_council_address: Default::default(),
                governance_min_delay: 0,
                // TODO verify
                bridgehub_create_new_chain_salt: rand::thread_rng().gen_range(0..=i64::MAX) as u64,
                validium_mode: chain_config.l1_batch_commit_data_generator_mode
                    == L1BatchCommitmentMode::Validium,
                validator_sender_operator_eth: wallets_config.operator.address,
                validator_sender_operator_blobs_eth: wallets_config.blob_operator.address,
                validator_sender_operator_prove: wallets_config
                    .prove_operator
                    .as_ref()
                    .map(|w| w.address),
                validator_sender_operator_execute: wallets_config
                    .execute_operator
                    .as_ref()
                    .map(|w| w.address),
                allow_evm_emulator: chain_config.evm_emulator,
            },
            owner_address: wallets_config.governor.address,
            contracts: Create2Addresses {
                create2_factory_addr,
                create2_factory_salt: H256::random(),
            },
            initialize_legacy_bridge,
        })
    }
}
