use ethers::types::Address;
use serde::{Deserialize, Serialize};
use types::BaseToken;
use zksync_basic_types::commitment::L1BatchCommitmentMode;

use crate::traits::ZkStackConfig;

#[derive(Debug, Deserialize, Serialize, Clone)]

struct ChainConfig {
    chain_id: u64,
    blob_operator: Address,
    operator: Address,
    governor: Address,
    base_token: BaseTokenInt,
    pubdata_pricing_mode: u8,
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
    chain: ChainConfig,
}

impl ProposeRegistrationInputConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        chain_registrar: Address,
        chain_id: u64,
        blob_operator: Address,
        operator: Address,
        governor: Address,
        token: BaseToken,
        token_multiplier_setter: Option<Address>,
        l1batch_commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        ProposeRegistrationInputConfig {
            chain_registrar,
            chain: ChainConfig {
                chain_id,
                blob_operator,
                operator,
                governor,
                base_token: BaseTokenInt {
                    token_multiplier_setter,
                    address: token.address,
                    nominator: token.nominator,
                    denominator: token.denominator,
                },
                pubdata_pricing_mode: l1batch_commitment_mode as u8,
            },
        }
    }
}

impl ZkStackConfig for ProposeRegistrationInputConfig {}
