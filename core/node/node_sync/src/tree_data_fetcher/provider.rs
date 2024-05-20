use std::fmt;

use async_trait::async_trait;
use zksync_types::{L1BatchNumber, H256};
use zksync_web3_decl::{
    client::{DynClient, L2},
    error::{ClientRpcContext, EnrichedClientResult},
    namespaces::ZksNamespaceClient,
};

#[derive(Debug, thiserror::Error)]
pub(crate) enum MissingData {
    /// The provider lacks a requested L1 batch.
    #[error("no requested L1 batch")]
    Batch,
    /// The provider lacks a root hash for a requested L1 batch; the batch itself is present on the provider.
    #[error("no root hash for L1 batch")]
    RootHash,
}

/// External provider of tree data, such as main node (via JSON-RPC).
#[async_trait]
pub(crate) trait TreeDataProvider: fmt::Debug + Send + Sync + 'static {
    /// Fetches a state root hash for the L1 batch with the specified number.
    async fn batch_details(
        &mut self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Result<H256, MissingData>>;
}

#[async_trait]
impl TreeDataProvider for Box<DynClient<L2>> {
    async fn batch_details(
        &mut self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Result<H256, MissingData>> {
        let Some(batch_details) = self
            .get_l1_batch_details(number)
            .rpc_context("get_l1_batch_details")
            .with_arg("number", &number)
            .await?
        else {
            return Ok(Err(MissingData::Batch));
        };
        Ok(batch_details.base.root_hash.ok_or(MissingData::RootHash))
    }
}
