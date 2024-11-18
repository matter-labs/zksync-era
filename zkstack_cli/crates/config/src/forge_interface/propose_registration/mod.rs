use crate::traits::ZkStackConfig;
use ethers::types::Address;
use serde::{Deserialize, Serialize};
use types::BaseToken;
use zksync_basic_types::commitment::L1BatchCommitmentMode;

#[derive(Debug, Deserialize, Serialize, Clone)]

struct ChainConfig {
    l2_chain_id: u64,
    blob_operator: Address,
    operator: Address,
    governor: Address,
    token: BaseTokenInt,
    l1batch_commitment_mode: L1BatchCommitmentMode,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct BaseTokenInt {
    pub token_multiplier_setter: Option<Address>,
    pub address: Address,
    pub nominator: u64,
    pub denominator: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProposeRegistrationInputConfig {
    chain_registrar: Address,
    chain_config: ChainConfig,
}

impl ProposeRegistrationInputConfig {
    pub fn new(
        chain_registrar: Address,
        l2_chain_id: u64,
        blob_operator: Address,
        operator: Address,
        governor: Address,
        token: BaseToken,
        token_multiplier_setter: Option<Address>,
        l1batch_commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        ProposeRegistrationInputConfig {
            chain_registrar,
            chain_config: ChainConfig {
                l2_chain_id,
                blob_operator,
                operator,
                governor,
                token: BaseTokenInt {
                    token_multiplier_setter,
                    address: token.address,
                    nominator: token.nominator,
                    denominator: token.denominator,
                },
                l1batch_commitment_mode,
            },
        }
    }
}

impl ZkStackConfig for ProposeRegistrationInputConfig {}
