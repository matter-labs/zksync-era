use zksync_dal::{transactions_dal::L2TxSubmissionResult, Connection, Core};
use zksync_multivm::interface::tracer::ValidationTraces;
use zksync_types::{
    api::{Transaction, TransactionDetails, TransactionId},
    l2::L2Tx,
    Address, Nonce, H256,
};
use zksync_web3_decl::error::Web3Error;

use super::SubmitTxError;
use crate::execution_sandbox::SandboxExecutionOutput;

/// An abstraction of "destination" for transactions that should be propagated to the mempool.
///
/// By default, `TxSender` always has access to the Postgres replica pool, but this only provides read-only progress.
/// However, `TxSender` should be able to propagate transactions as well, and for that purpose the implementation may
/// be different. For example, main node has access to the master pool, and external node has a proxy that submits
/// transaction to the main node.
///
/// Both approaches represent different implementations of `TxSink` trait.
///
/// Additionally, `TxSink` may be stateful: e.g. if the effects of transaction submission are not immediately visible
/// through the replica pool, `TxSink` may implement methods to allow cache-like lookups. These methods are not mandatory
/// and may be implemented as no-ops.
#[async_trait::async_trait]
pub trait TxSink: std::fmt::Debug + Send + Sync + 'static {
    /// Ensures that transaction is propagated to the mempool.
    async fn submit_tx(
        &self,
        tx: &L2Tx,
        execution_output: &SandboxExecutionOutput,
        validation_traces: ValidationTraces,
    ) -> Result<L2TxSubmissionResult, SubmitTxError>;

    /// Attempts to look up the pending nonce for the account in the sink-specific storage.
    /// By default, returns `Ok(None)`.
    async fn lookup_pending_nonce(
        &self,
        _account_address: Address,
        _last_known_nonce: u32,
    ) -> Result<Option<Nonce>, Web3Error> {
        Ok(None)
    }

    /// Attempts to look up the transaction by its API ID in the sink-specific storage.
    /// By default, returns `Ok(None)`.
    async fn lookup_tx(
        &self,
        _storage: &mut Connection<'_, Core>,
        _id: TransactionId,
    ) -> Result<Option<Transaction>, Web3Error> {
        Ok(None)
    }

    /// Attempts to look up the transaction details by its hash in the sink-specific storage.
    /// By default, returns `Ok(None)`.
    async fn lookup_tx_details(
        &self,
        _storage: &mut Connection<'_, Core>,
        _hash: H256,
    ) -> Result<Option<TransactionDetails>, Web3Error> {
        Ok(None)
    }
}
