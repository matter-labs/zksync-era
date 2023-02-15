use crate::api_server::web3::namespaces::web3::Web3Namespace;
use zksync_web3_decl::{jsonrpsee::core::RpcResult, namespaces::web3::Web3NamespaceServer};

impl Web3NamespaceServer for Web3Namespace {
    fn client_version(&self) -> RpcResult<String> {
        Ok(self.client_version_impl())
    }
}
