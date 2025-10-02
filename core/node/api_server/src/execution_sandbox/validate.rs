use std::collections::HashSet;

use anyhow::Context as _;
use tracing::Instrument;
use zksync_dal::{Connection, Core, CoreDal};
use zksync_multivm::interface::{
    storage::StorageWithOverrides,
    tracer::{
        TimestampAsserterParams, ValidationError as RawValidationError, ValidationParams,
        ValidationTraces,
    },
};
use zksync_types::{
    fee_model::BatchFeeInput, l2::L2Tx, Address, TRUSTED_ADDRESS_SLOTS, TRUSTED_TOKEN_SLOTS,
};

use super::{
    execute::{SandboxAction, SandboxExecutor},
    vm_metrics::{SandboxStage, EXECUTION_METRICS, SANDBOX_METRICS},
    BlockArgs, VmPermit,
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

impl SandboxExecutor {
    #[tracing::instrument(level = "debug", skip_all)]
    pub(crate) async fn validate_tx_in_sandbox(
        &self,
        vm_permit: VmPermit,
        mut connection: Connection<'static, Core>,
        tx: L2Tx,
        block_args: BlockArgs,
        fee_input: BatchFeeInput,
        whitelisted_tokens_for_aa: &[Address],
    ) -> Result<ValidationTraces, ValidationError> {
        let total_latency = SANDBOX_METRICS.sandbox[&SandboxStage::ValidateInSandbox].start();
        let validation_params = get_validation_params(
            &mut connection,
            &tx,
            self.options.eth_call.validation_computational_gas_limit(),
            whitelisted_tokens_for_aa,
            self.timestamp_asserter_params.clone(),
        )
        .await
        .context("failed getting validation params")?;

        let action = SandboxAction::Execution { fee_input, tx };
        let (env, storage) = self
            .prepare_env_and_storage(connection, &block_args, &action)
            .await?;
        let SandboxAction::Execution { tx, .. } = action else {
            unreachable!(); // by construction
        };
        let storage = StorageWithOverrides::new(storage);

        let stage_latency = SANDBOX_METRICS.sandbox[&SandboxStage::Validation].start();
        let validation_result = self
            .engine
            .validate_transaction(storage, env, tx, validation_params)
            .instrument(tracing::debug_span!("validation"))
            .await?;
        drop(vm_permit);
        stage_latency.observe();

        total_latency.observe();
        validation_result.map_err(ValidationError::Vm)
    }
}

/// Some slots can be marked as "trusted". That is needed for slots which can not be
/// trusted to change between validation and execution in general case, but
/// sometimes we can safely rely on them to not change often.
pub(super) async fn get_validation_params(
    connection: &mut Connection<'_, Core>,
    tx: &L2Tx,
    computational_gas_limit: u32,
    whitelisted_tokens_for_aa: &[Address],
    timestamp_asserter_params: Option<TimestampAsserterParams>,
) -> anyhow::Result<ValidationParams> {
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
    Ok(ValidationParams {
        user_address,
        paymaster_address,
        trusted_slots,
        trusted_addresses,
        trusted_address_slots,
        computational_gas_limit,
        timestamp_asserter_params,
    })
}
