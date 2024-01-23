use std::collections::HashSet;

use anyhow::Context as _;
use multivm::{
    interface::{ExecutionResult, VmExecutionMode, VmInterface},
    tracers::{
        validator::{self, ValidationTracer, ValidationTracerParams},
        StorageInvocations,
    },
    vm_latest::HistoryDisabled,
    MultiVMTracer,
};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{l2::L2Tx, Transaction, TRUSTED_ADDRESS_SLOTS, TRUSTED_TOKEN_SLOTS};

use super::{
    apply,
    execute::TransactionExecutor,
    vm_metrics::{SandboxStage, EXECUTION_METRICS, SANDBOX_METRICS},
    BlockArgs, TxExecutionArgs, TxSharedArgs, VmPermit,
};

/// Validation error used by the sandbox. Besides validation errors returned by VM, it also includes an internal error
/// variant (e.g., for DB-related errors).
#[derive(Debug, thiserror::Error)]
pub(crate) enum ValidationError {
    #[error("VM validation error: {0}")]
    Vm(validator::ValidationError),
    #[error("Internal error")]
    Internal(#[from] anyhow::Error),
}

impl TransactionExecutor {
    pub(crate) async fn validate_tx_in_sandbox(
        &self,
        connection_pool: ConnectionPool,
        vm_permit: VmPermit,
        tx: L2Tx,
        shared_args: TxSharedArgs,
        block_args: BlockArgs,
        computational_gas_limit: u32,
    ) -> Result<(), ValidationError> {
        #[cfg(test)]
        if let Self::Mock(mock) = self {
            return mock.validate_tx(&tx);
        }

        let stage_latency = SANDBOX_METRICS.sandbox[&SandboxStage::ValidateInSandbox].start();
        let mut connection = connection_pool
            .access_storage_tagged("api")
            .await
            .context("failed acquiring DB connection")?;
        let validation_params =
            get_validation_params(&mut connection, &tx, computational_gas_limit)
                .await
                .context("failed getting validation params")?;
        drop(connection);

        let execution_args = TxExecutionArgs::for_validation(&tx);
        let tx: Transaction = tx.into();

        let validation_result = tokio::task::spawn_blocking(move || {
            let span = tracing::debug_span!("validate_in_sandbox").entered();
            let result = apply::apply_vm_in_sandbox(
                vm_permit,
                shared_args,
                true,
                &execution_args,
                &connection_pool,
                tx,
                block_args,
                |vm, tx| {
                    let stage_latency = SANDBOX_METRICS.sandbox[&SandboxStage::Validation].start();
                    let span = tracing::debug_span!("validation").entered();
                    vm.push_transaction(tx);

                    let (tracer, validation_result) =
                        ValidationTracer::<HistoryDisabled>::new(validation_params);

                    let result = vm.inspect(
                        vec![
                            tracer.into_tracer_pointer(),
                            StorageInvocations::new(execution_args.missed_storage_invocation_limit)
                                .into_tracer_pointer(),
                        ]
                        .into(),
                        VmExecutionMode::OneTx,
                    );

                    let result = match (result.result, validation_result.get()) {
                        (_, Some(err)) => {
                            Err(validator::ValidationError::ViolatedRule(err.clone()))
                        }
                        (ExecutionResult::Halt { reason }, _) => {
                            Err(validator::ValidationError::FailedTx(reason))
                        }
                        (_, None) => Ok(()),
                    };

                    stage_latency.observe();
                    span.exit();
                    result
                },
            );
            span.exit();
            result
        })
        .await
        .context("transaction validation panicked")??;

        stage_latency.observe();
        validation_result.map_err(ValidationError::Vm)
    }
}

/// Some slots can be marked as "trusted". That is needed for slots which can not be
/// trusted to change between validation and execution in general case, but
/// sometimes we can safely rely on them to not change often.
async fn get_validation_params(
    connection: &mut StorageProcessor<'_>,
    tx: &L2Tx,
    computational_gas_limit: u32,
) -> anyhow::Result<ValidationTracerParams> {
    let method_latency = EXECUTION_METRICS.get_validation_params.start();
    let user_address = tx.common_data.initiator_address;
    let paymaster_address = tx.common_data.paymaster_params.paymaster;

    // This method assumes that the number of tokens is relatively low. When it grows
    // we may need to introduce some kind of caching.
    let all_tokens = connection
        .tokens_dal()
        .get_all_l2_token_addresses()
        .await
        .context("failed getting addresses of L2 tokens")?;
    EXECUTION_METRICS.tokens_amount.set(all_tokens.len());

    let span = tracing::debug_span!("compute_trusted_slots_for_validation").entered();
    let trusted_slots: HashSet<_> = all_tokens
        .iter()
        .flat_map(|&token| TRUSTED_TOKEN_SLOTS.iter().map(move |&slot| (token, slot)))
        .collect();

    // We currently don't support any specific trusted addresses.
    let trusted_addresses = HashSet::new();

    // The slots the value of which will be added as allowed address on the fly.
    // Required for working with transparent proxies.
    let trusted_address_slots: HashSet<_> = all_tokens
        .into_iter()
        .flat_map(|token| TRUSTED_ADDRESS_SLOTS.iter().map(move |&slot| (token, slot)))
        .collect();
    EXECUTION_METRICS
        .trusted_address_slots_amount
        .set(trusted_address_slots.len());
    span.exit();

    method_latency.observe();
    Ok(ValidationTracerParams {
        user_address,
        paymaster_address,
        trusted_slots,
        trusted_addresses,
        trusted_address_slots,
        computational_gas_limit,
    })
}
