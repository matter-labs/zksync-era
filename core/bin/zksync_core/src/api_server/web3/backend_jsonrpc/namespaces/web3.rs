// Built-in uses

// External uses
use jsonrpc_core::Result;
use jsonrpc_derive::rpc;

// Workspace uses

// Local uses
use crate::web3::namespaces::Web3Namespace;

#[rpc]
pub trait Web3NamespaceT {
    #[rpc(name = "web3_clientVersion", returns = "String")]
    fn client_version(&self) -> Result<String>;
}

impl Web3NamespaceT for Web3Namespace {
    fn client_version(&self) -> Result<String> {
        Ok(self.client_version_impl())
    }
}
