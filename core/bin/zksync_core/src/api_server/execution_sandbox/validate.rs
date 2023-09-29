use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

use vm::oracles::tracer::{ValidationError, ValidationTracerParams};
use vm::vm_with_bootloader::push_transaction_to_bootloader_memory;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{l2::L2Tx, Transaction, TRUSTED_ADDRESS_SLOTS, TRUSTED_TOKEN_SLOTS, U256};

use super::{
    adjust_l1_gas_price_for_tx, apply, BlockArgs, TxExecutionArgs, TxSharedArgs, VmPermit,
};

impl TxSharedArgs {
    pub async fn validate_tx_with_pending_state(
        mut self,
        vm_permit: VmPermit,
        connection_pool: ConnectionPool,
        tx: L2Tx,
        computational_gas_limit: u32,
    ) -> Result<(), ValidationError> {
        let mut connection = connection_pool.access_storage_tagged("api").await;
        let block_args = BlockArgs::pending(&mut connection).await;
        drop(connection);
        self.adjust_l1_gas_price(tx.common_data.fee.gas_per_pubdata_limit);
        self.validate_tx_in_sandbox(
            connection_pool,
            vm_permit,
            tx,
            block_args,
            computational_gas_limit,
        )
        .await
    }

    // In order for validation to pass smoothlessly, we need to ensure that block's required gasPerPubdata will be
    // <= to the one in the transaction itself.
    pub fn adjust_l1_gas_price(&mut self, gas_per_pubdata_limit: U256) {
        self.l1_gas_price = adjust_l1_gas_price_for_tx(
            self.l1_gas_price,
            self.fair_l2_gas_price,
            gas_per_pubdata_limit,
        );
    }

    async fn validate_tx_in_sandbox(
        self,
        connection_pool: ConnectionPool,
        vm_permit: VmPermit,
        tx: L2Tx,
        block_args: BlockArgs,
        computational_gas_limit: u32,
    ) -> Result<(), ValidationError> {
        let stage_started_at = Instant::now();
        let mut connection = connection_pool.access_storage_tagged("api").await;
        let validation_params =
            get_validation_params(&mut connection, &tx, computational_gas_limit).await;
        drop(connection);

        let execution_args = TxExecutionArgs::for_validation(&tx);
        let execution_mode = execution_args.execution_mode;
        let tx: Transaction = tx.into();
        let (validation_result, _) = tokio::task::spawn_blocking(move || {
            let span = tracing::debug_span!("validate_in_sandbox").entered();
            let result = apply::apply_vm_in_sandbox(
                vm_permit,
                self,
                &execution_args,
                &connection_pool,
                tx,
                block_args,
                HashMap::new(),
                |vm, tx| {
                    let stage_started_at = Instant::now();
                    let span = tracing::debug_span!("validation").entered();
                    push_transaction_to_bootloader_memory(vm, &tx, execution_mode, None);
                    let result = vm.execute_validation(validation_params);

                    metrics::histogram!("api.web3.sandbox", stage_started_at.elapsed(), "stage" => "validation");
                    span.exit();
                    result
                },
            );
            span.exit();
            result
        }).await.unwrap();

        metrics::histogram!("server.api.validation_sandbox", stage_started_at.elapsed(), "stage" => "validate_in_sandbox");
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
    let start_time = Instant::now();
    let user_address = tx.common_data.initiator_address;
    let paymaster_address = tx.common_data.paymaster_params.paymaster;

    // This method assumes that the number of tokens is relatively low. When it grows
    // we may need to introduce some kind of caching.
    let all_tokens = connection.tokens_dal().get_all_l2_token_addresses().await;
    metrics::gauge!("api.execution.tokens.amount", all_tokens.len() as f64);

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

    metrics::gauge!(
        "api.execution.trusted_address_slots.amount",
        trusted_address_slots.len() as f64
    );
    span.exit();

    metrics::histogram!("api.execution.get_validation_params", start_time.elapsed());
    ValidationTracerParams {
        user_address,
        paymaster_address,
        trusted_slots,
        trusted_addresses,
        trusted_address_slots,
        computational_gas_limit,
    }
}
