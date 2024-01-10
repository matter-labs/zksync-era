use std::collections::HashSet;

use multivm::{
    interface::{ExecutionResult, VmExecutionMode, VmInterface},
    tracers::{
        validator::{ValidationError, ValidationTracer, ValidationTracerParams},
        StorageInvocations,
    },
    vm_latest::HistoryDisabled,
    MultiVMTracer,
};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{l2::L2Tx, Transaction, TRUSTED_ADDRESS_SLOTS, TRUSTED_TOKEN_SLOTS};

use super::{
    apply,
    vm_metrics::{SandboxStage, EXECUTION_METRICS, SANDBOX_METRICS},
    BlockArgs, TxExecutionArgs, TxSharedArgs, VmPermit,
};

impl TxSharedArgs {
    pub(crate) async fn validate_tx_in_sandbox(
        self,
        connection_pool: ConnectionPool,
        vm_permit: VmPermit,
        tx: L2Tx,
        block_args: BlockArgs,
        computational_gas_limit: u32,
    ) -> Result<(), ValidationError> {
        let stage_latency = SANDBOX_METRICS.sandbox[&SandboxStage::ValidateInSandbox].start();
        let mut connection = connection_pool.access_storage_tagged("api").await.unwrap();
        let validation_params =
            get_validation_params(&mut connection, &tx, computational_gas_limit).await;
        drop(connection);

        let execution_args = TxExecutionArgs::for_validation(&tx);
        let tx: Transaction = tx.into();

        let validation_result = tokio::task::spawn_blocking(move || {
            let span = tracing::debug_span!("validate_in_sandbox").entered();
            let result = apply::apply_vm_in_sandbox(
                vm_permit,
                self,
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
                        (_, Some(err)) => Err(ValidationError::ViolatedRule(err.clone())),
                        (ExecutionResult::Halt { reason }, _) => {
                            Err(ValidationError::FailedTx(reason))
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
        .unwrap();

        stage_latency.observe();
        validation_result
    }
}

// Some slots can be marked as "trusted". That is needed for slots which can not be
// trusted to change between validation and execution in general case, but
// sometimes we can safely rely on them to not change often.
async fn get_validation_params(
    connection: &mut StorageProcessor<'_>,
    tx: &L2Tx,
    computational_gas_limit: u32,
) -> ValidationTracerParams {
    let method_latency = EXECUTION_METRICS.get_validation_params.start();
    let user_address = tx.common_data.initiator_address;
    let paymaster_address = tx.common_data.paymaster_params.paymaster;

    // This method assumes that the number of tokens is relatively low. When it grows
    // we may need to introduce some kind of caching.
    let all_tokens = connection.tokens_dal().get_all_l2_token_addresses().await;
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
    ValidationTracerParams {
        user_address,
        paymaster_address,
        trusted_slots,
        trusted_addresses,
        trusted_address_slots,
        computational_gas_limit,
    }
}
