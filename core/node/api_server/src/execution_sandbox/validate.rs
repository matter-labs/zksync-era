use std::collections::HashSet;

use anyhow::Context as _;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_multivm::{
    interface::ExecutionResult,
    tracers::{ValidationError as RawValidationError, ValidationTracerParams},
};
use zksync_types::{
    api::state_override::StateOverride, l2::L2Tx, Address, Transaction, TRUSTED_ADDRESS_SLOTS,
    TRUSTED_TOKEN_SLOTS,
};

use super::{
    apply::Sandbox,
    execute::TransactionExecutor,
    vm_metrics::{SandboxStage, EXECUTION_METRICS, SANDBOX_METRICS},
    ApiTracer, BlockArgs, TxExecutionArgs, TxSharedArgs, VmPermit,
};

/// Validation error used by the sandbox. Besides validation errors returned by VM, it also includes an internal error
/// variant (e.g., for DB-related errors).
#[derive(Debug, thiserror::Error)]
pub(crate) enum ValidationError {
    #[error("VM validation error: {0}")]
    Vm(RawValidationError),
    #[error("Internal error")]
    Internal(#[from] anyhow::Error),
}

impl TransactionExecutor {
    #[tracing::instrument(level = "debug", skip_all)]
    pub(crate) async fn validate_tx_in_sandbox(
        &self,
        mut connection: Connection<'static, Core>,
        vm_permit: VmPermit,
        tx: L2Tx,
        shared_args: TxSharedArgs,
        block_args: BlockArgs,
        computational_gas_limit: u32,
    ) -> Result<(), ValidationError> {
        if let Self::Mock(mock) = self {
            return mock.validate_tx(tx, &block_args);
        }

        let stage_latency = SANDBOX_METRICS.sandbox[&SandboxStage::ValidateInSandbox].start();
        let params = get_validation_params(
            &mut connection,
            &tx,
            computational_gas_limit,
            &shared_args.whitelisted_tokens_for_aa,
        )
        .await
        .context("failed getting validation params")?;

        let execution_args = TxExecutionArgs::for_validation(&tx);
        let tx: Transaction = tx.into();

        let sandbox = Sandbox::new(
            vm_permit,
            connection,
            shared_args,
            execution_args,
            block_args,
            &StateOverride::default(),
        )
        .await?;

        let validation_result = tokio::task::spawn_blocking(|| {
            let vm = sandbox.build_vm(tx, true);
            let (tracer, validation_result) = ApiTracer::validation(params);

            let stage_latency = SANDBOX_METRICS.sandbox[&SandboxStage::Validation].start();
            let span = tracing::debug_span!("validation").entered();
            let result = vm.inspect_transaction(vec![tracer]);
            stage_latency.observe();
            span.exit();

            match (result.result, validation_result.get()) {
                (_, Some(rule)) => Err(RawValidationError::ViolatedRule(rule.clone())),
                (ExecutionResult::Halt { reason }, _) => Err(RawValidationError::FailedTx(reason)),
                (_, None) => Ok(()),
            }
        })
        .await
        .context("VM execution panicked")?;

        stage_latency.observe();
        validation_result.map_err(ValidationError::Vm)
    }
}

/// Some slots can be marked as "trusted". That is needed for slots which can not be
/// trusted to change between validation and execution in general case, but
/// sometimes we can safely rely on them to not change often.
async fn get_validation_params(
    connection: &mut Connection<'_, Core>,
    tx: &L2Tx,
    computational_gas_limit: u32,
    whitelisted_tokens_for_aa: &[Address],
) -> anyhow::Result<ValidationTracerParams> {
    let method_latency = EXECUTION_METRICS.get_validation_params.start();
    let user_address = tx.common_data.initiator_address;
    let paymaster_address = tx.common_data.paymaster_params.paymaster;

    // This method assumes that the number of tokens is relatively low. When it grows
    // we may need to introduce some kind of caching.
    let all_bridged_tokens = connection.tokens_dal().get_all_l2_token_addresses().await?;
    let all_tokens: Vec<_> = all_bridged_tokens
        .iter()
        .chain(whitelisted_tokens_for_aa)
        .collect();
    EXECUTION_METRICS.tokens_amount.set(all_tokens.len());

    let span = tracing::debug_span!("compute_trusted_slots_for_validation").entered();
    let trusted_slots: HashSet<_> = all_tokens
        .iter()
        .flat_map(|&token| TRUSTED_TOKEN_SLOTS.iter().map(move |&slot| (*token, slot)))
        .collect();

    // We currently don't support any specific trusted addresses.
    let trusted_addresses = HashSet::new();

    // The slots the value of which will be added as allowed address on the fly.
    // Required for working with transparent proxies.
    let trusted_address_slots: HashSet<_> = all_tokens
        .into_iter()
        .flat_map(|token| {
            TRUSTED_ADDRESS_SLOTS
                .iter()
                .map(move |&slot| (*token, slot))
        })
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
