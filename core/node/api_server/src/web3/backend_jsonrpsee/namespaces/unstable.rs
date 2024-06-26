use zksync_types::{api::ApiTransactionExecutionInfo, H256};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::UnstableNamespaceServer,
};

use crate::web3::namespaces::UnstableNamespace;

#[async_trait]
impl UnstableNamespaceServer for UnstableNamespace {
    async fn transaction_execution_info(
        &self,
        hash: H256,
    ) -> RpcResult<ApiTransactionExecutionInfo> {
        self.transaction_execution_info_impl(hash)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }
}
