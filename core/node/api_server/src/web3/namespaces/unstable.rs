use chrono::{DateTime, Utc};
use zksync_dal::{CoreDal, DalError};
use zksync_types::{
    api::{TeeProof, TransactionExecutionInfo},
    tee_types::TeeType,
    L1BatchNumber,
};
use zksync_web3_decl::{error::Web3Error, types::H256};

use crate::web3::{backend_jsonrpsee::MethodTracer, RpcState};

#[derive(Debug)]
pub(crate) struct UnstableNamespace {
    state: RpcState,
}

impl UnstableNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    pub(crate) fn current_method(&self) -> &MethodTracer {
        &self.state.current_method
    }

    pub async fn transaction_execution_info_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionExecutionInfo>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        Ok(storage
            .transactions_web3_dal()
            .get_unstable_transaction_execution_info(hash)
            .await
            .map_err(DalError::generalize)?
            .map(|execution_info| TransactionExecutionInfo { execution_info }))
    }

    pub async fn get_tee_proofs_impl(
        &self,
        l1_batch_number: L1BatchNumber,
        tee_type: Option<TeeType>,
    ) -> Result<Vec<TeeProof>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        Ok(storage
            .tee_proof_generation_dal()
            .get_tee_proofs(l1_batch_number, tee_type)
            .await
            .map_err(DalError::generalize)?
            .into_iter()
            .map(|proof| TeeProof {
                l1_batch_number,
                tee_type,
                pubkey: proof.pubkey,
                signature: proof.signature,
                proof: proof.proof,
                proved_at: DateTime::<Utc>::from_naive_utc_and_offset(proof.updated_at, Utc),
                attestation: proof.attestation,
            })
            .collect::<Vec<_>>())
    }
}
