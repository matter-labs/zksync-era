use crate::api_server::web3::get_config;

use zksync_types::U256;

#[derive(Debug, Clone)]
pub struct NetNamespace;

impl NetNamespace {
    pub fn version_impl(&self) -> String {
        get_config().chain.eth.zksync_network_id.to_string()
    }

    pub fn peer_count_impl(&self) -> U256 {
        0.into()
    }

    pub fn is_listening_impl(&self) -> bool {
        false
    }
}
