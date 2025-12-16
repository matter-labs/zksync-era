use serde::{Deserialize, Serialize};
use zksync_basic_types::{Address, H256};

pub mod accept_ownership;
pub mod deploy_ecosystem;
pub mod deploy_gateway_tx_filterer;
pub mod deploy_l2_contracts;
pub mod gateway_preparation;
pub mod gateway_vote_preparation;
pub mod paymaster;
pub mod register_chain;
pub mod script_params;
pub mod upgrade_ecosystem;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Create2Addresses {
    pub create2_factory_addr: Address,
    pub create2_factory_salt: H256,
}
