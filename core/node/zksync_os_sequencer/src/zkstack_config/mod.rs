use crate::zkstack_config::contracts::ContractsConfig;
use crate::zkstack_config::genesis::GenesisConfig;
use crate::zkstack_config::wallets::WalletsConfig;

pub mod contracts;
pub mod genesis;
pub mod wallets;

#[derive(Clone, Debug)]
pub struct ZkstackConfig {
    pub contracts: ContractsConfig,
    pub genesis: GenesisConfig,
    pub wallets: WalletsConfig,
}
