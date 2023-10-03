use zksync_types::U256;
use zksync_web3_decl::{jsonrpsee::core::RpcResult, namespaces::net::NetNamespaceServer};

use crate::api_server::web3::NetNamespace;

impl NetNamespaceServer for NetNamespace {
    fn version(&self) -> RpcResult<String> {
        Ok(self.version_impl())
    }

    fn peer_count(&self) -> RpcResult<U256> {
        Ok(self.peer_count_impl())
    }

    fn is_listening(&self) -> RpcResult<bool> {
        Ok(self.is_listening_impl())
    }
}
