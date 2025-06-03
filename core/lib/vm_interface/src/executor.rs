//! High-level executor traits.

use std::fmt;

use async_trait::async_trait;
use zksync_types::{commitment::PubdataParams, l2::L2Tx, Transaction};

use crate::{
    storage::{ReadStorage, StorageView},
    tracer::{ValidationError, ValidationParams, ValidationTraces},
    BatchTransactionExecutionResult, FinishedL1Batch, L1BatchEnv, L2BlockEnv, OneshotEnv,
    OneshotTracingParams, OneshotTransactionExecutionResult, SystemEnv,
    TransactionExecutionMetrics, TxExecutionArgs,
};

/// Factory of [`BatchExecutor`]s.
pub trait BatchExecutorFactory<S: Send + 'static>: 'static + Send + fmt::Debug {
    /// Initializes an executor for a batch with the specified params and using the provided storage.
    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
        pubdata_params: PubdataParams,
    ) -> Box<dyn BatchExecutor<S>>;
}

/// Handle for executing a single L1 batch.
///
/// The handle is parametric by the transaction execution output in order to be able to represent different
/// levels of abstraction.
#[async_trait]
pub trait BatchExecutor<S>: 'static + Send + fmt::Debug {
    /// Executes a transaction.
    async fn execute_tx(
        &mut self,
        tx: Transaction,
    ) -> anyhow::Result<BatchTransactionExecutionResult>;

    /// Rolls back the last executed transaction.
    async fn rollback_last_tx(&mut self) -> anyhow::Result<()>;

    /// Starts a next L2 block with the specified params.
    async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()>;

    /// Finished the current L1 batch.
    async fn finish_batch(self: Box<Self>) -> anyhow::Result<(FinishedL1Batch, StorageView<S>)>;

    /// Return gas remaining in VM instance.
    async fn gas_remaining(&mut self) -> anyhow::Result<u32>;

    /// Rolls back the last executed l2 block.
    async fn rollback_l2_block(&mut self) -> anyhow::Result<()>;
}

/// VM executor capable of executing isolated transactions / calls (as opposed to [batch execution](BatchExecutor)).
#[async_trait]
pub trait OneshotExecutor<S: ReadStorage> {
    /// Executes a transaction or call with optional tracers.
    async fn inspect_transaction_with_bytecode_compression(
        &self,
        storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        tracing: OneshotTracingParams,
    ) -> anyhow::Result<OneshotTransactionExecutionResult>;
}

/// VM executor capable of validating transactions.
#[async_trait]
pub trait TransactionValidator<S: ReadStorage> {
    /// Validates the provided transaction.
    async fn validate_transaction(
        &self,
        storage: S,
        env: OneshotEnv,
        tx: L2Tx,
        validation_params: ValidationParams,
    ) -> anyhow::Result<Result<ValidationTraces, ValidationError>>;
}

/// Generic transaction filter.
// TODO: can be used for initiator allowlist
#[async_trait]
pub trait TransactionFilter: fmt::Debug + Send + Sync {
    /// Performs checks on the provided transaction. Returns an error (a human-readable message) if the transaction execution
    /// does not satisfy this filter.
    async fn filter_transaction(
        &self,
        transaction: &Transaction,
        metrics: &TransactionExecutionMetrics,
    ) -> Result<(), String>;
}

/// Filter that always succeeds.
#[async_trait]
impl TransactionFilter for () {
    async fn filter_transaction(
        &self,
        _transaction: &Transaction,
        _metrics: &TransactionExecutionMetrics,
    ) -> Result<(), String> {
        Ok(())
    }
}
