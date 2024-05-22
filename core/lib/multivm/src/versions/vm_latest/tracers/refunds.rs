use std::marker::PhantomData;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};
use zk_evm_1_5_0::{
    aux_structures::Timestamp,
    tracing::{BeforeExecutionData, VmLocalStateData},
};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_types::{H256, U256};
use zksync_utils::ceil_div_u256;

use crate::{
    interface::{
        traits::tracers::dyn_tracers::vm_1_5_0::DynTracer, types::tracer::TracerExecutionStatus,
        L1BatchEnv, Refunds,
    },
    vm_latest::{
        bootloader_state::BootloaderState,
        constants::{BOOTLOADER_HEAP_PAGE, OPERATOR_REFUNDS_OFFSET, TX_GAS_LIMIT_OFFSET},
        old_vm::{history_recorder::HistoryMode, memory::SimpleMemory},
        tracers::{
            traits::VmTracer,
            utils::{get_vm_hook_params, VmHook},
        },
        types::internals::ZkSyncVmState,
        utils::fee::get_batch_base_fee,
        vm::MultiVMSubversion,
    },
};

#[derive(Debug, Clone, Copy)]
struct RefundRequest {
    refund: u64,
    gas_spent_on_pubdata: u64,
    used_gas_per_pubdata_byte: u32,
}

/// Tracer responsible for collecting information about refunds.
#[derive(Debug, Clone)]
pub(crate) struct RefundsTracer<S> {
    // Some(x) means that the bootloader has asked the operator
    // to provide the refund the user, where `x` is the refund proposed
    // by the bootloader itself.
    pending_refund_request: Option<RefundRequest>,
    refund_gas: u64,
    operator_refund: Option<u64>,
    timestamp_initial: Timestamp,
    timestamp_before_cycle: Timestamp,
    computational_gas_remaining_before: u32,
    spent_pubdata_counter_before: u32,
    l1_batch: L1BatchEnv,
    pubdata_published: u32,
    subversion: MultiVMSubversion,
    _phantom: PhantomData<S>,
}

impl<S> RefundsTracer<S> {
    pub(crate) fn new(l1_batch: L1BatchEnv, subversion: MultiVMSubversion) -> Self {
        Self {
            pending_refund_request: None,
            refund_gas: 0,
            operator_refund: None,
            timestamp_initial: Timestamp(0),
            timestamp_before_cycle: Timestamp(0),
            computational_gas_remaining_before: 0,
            spent_pubdata_counter_before: 0,
            l1_batch,
            pubdata_published: 0,
            subversion,
            _phantom: PhantomData,
        }
    }
}

impl<S> RefundsTracer<S> {
    fn requested_refund(&self) -> Option<RefundRequest> {
        self.pending_refund_request
    }

    fn set_refund_as_done(&mut self) {
        self.pending_refund_request = None;
    }

    fn block_overhead_refund(&mut self) -> u64 {
        0
    }

    pub(crate) fn get_refunds(&self) -> Refunds {
        Refunds {
            gas_refunded: self.refund_gas,
            operator_suggested_refund: self.operator_refund.unwrap_or_default(),
        }
    }

    pub(crate) fn tx_body_refund(
        &self,
        bootloader_refund: u64,
        gas_spent_on_pubdata: u64,
        tx_gas_limit: u64,
        current_ergs_per_pubdata_byte: u32,
        pubdata_published: u32,
        tx_hash: H256,
    ) -> u64 {
        let total_gas_spent = tx_gas_limit - bootloader_refund;

        let gas_spent_on_computation = total_gas_spent
            .checked_sub(gas_spent_on_pubdata)
            .unwrap_or_else(|| {
                tracing::error!(
                    "Gas spent on pubdata is greater than total gas spent. On pubdata: {}, total: {}",
                    gas_spent_on_pubdata,
                    total_gas_spent
                );
                0
            });

        // For now, bootloader charges only for base fee.
        let effective_gas_price = get_batch_base_fee(&self.l1_batch);

        let bootloader_eth_price_per_pubdata_byte =
            U256::from(effective_gas_price) * U256::from(current_ergs_per_pubdata_byte);

        let fair_eth_price_per_pubdata_byte =
            U256::from(self.l1_batch.fee_input.fair_pubdata_price());

        // For now, L1 originated transactions are allowed to pay less than fair fee per pubdata,
        // so we should take it into account.
        let eth_price_per_pubdata_byte_for_calculation = std::cmp::min(
            bootloader_eth_price_per_pubdata_byte,
            fair_eth_price_per_pubdata_byte,
        );

        let fair_fee_eth = U256::from(gas_spent_on_computation)
            * U256::from(self.l1_batch.fee_input.fair_l2_gas_price())
            + U256::from(pubdata_published) * eth_price_per_pubdata_byte_for_calculation;
        let pre_paid_eth = U256::from(tx_gas_limit) * U256::from(effective_gas_price);
        let refund_eth = pre_paid_eth.checked_sub(fair_fee_eth).unwrap_or_else(|| {
            tracing::error!(
                "Fair fee is greater than pre paid. Fair fee: {} wei, pre paid: {} wei",
                fair_fee_eth,
                pre_paid_eth
            );
            U256::zero()
        });

        tracing::trace!(
            "Fee benchmark for transaction with hash {}",
            hex::encode(tx_hash.as_bytes())
        );
        tracing::trace!("Gas Limit: {}", tx_gas_limit);
        tracing::trace!("Gas spent on computation: {}", gas_spent_on_computation);
        tracing::trace!("Gas spent on pubdata: {}", gas_spent_on_pubdata);
        tracing::trace!("Pubdata published: {}", pubdata_published);

        ceil_div_u256(refund_eth, effective_gas_price.into()).as_u64()
    }

    pub(crate) fn pubdata_published(&self) -> u32 {
        self.pubdata_published
    }
}

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for RefundsTracer<S> {
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        self.timestamp_before_cycle = Timestamp(state.vm_local_state.timestamp);
        let hook = VmHook::from_opcode_memory(&state, &data, self.subversion);
        match hook {
            VmHook::NotifyAboutRefund => {
                self.refund_gas = get_vm_hook_params(memory, self.subversion)[0].as_u64()
            }
            VmHook::AskOperatorForRefund => {
                self.pending_refund_request = Some(RefundRequest {
                    refund: get_vm_hook_params(memory, self.subversion)[0].as_u64(),
                    gas_spent_on_pubdata: get_vm_hook_params(memory, self.subversion)[1].as_u64(),
                    used_gas_per_pubdata_byte: get_vm_hook_params(memory, self.subversion)[2]
                        .as_u32(),
                })
            }
            _ => {}
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for RefundsTracer<S> {
    fn initialize_tracer(&mut self, state: &mut ZkSyncVmState<S, H>) {
        self.timestamp_initial = Timestamp(state.local_state.timestamp);
        self.computational_gas_remaining_before =
            state.local_state.callstack.current.ergs_remaining;
        self.spent_pubdata_counter_before = state.local_state.pubdata_revert_counter.0 as u32;
    }

    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
        #[metrics(label = "type", rename_all = "snake_case")]
        enum RefundType {
            Bootloader,
            Operator,
        }

        const PERCENT_BUCKETS: Buckets = Buckets::values(&[
            5.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 120.0,
        ]);

        #[derive(Debug, Metrics)]
        #[metrics(prefix = "vm")]
        struct RefundMetrics {
            #[metrics(buckets = PERCENT_BUCKETS)]
            refund: Family<RefundType, Histogram<f64>>,
            #[metrics(buckets = PERCENT_BUCKETS)]
            refund_diff: Histogram<f64>,
        }

        #[vise::register]
        static METRICS: vise::Global<RefundMetrics> = vise::Global::new();

        // This means that the bootloader has informed the system (usually via `VMHooks`) - that some gas
        // should be refunded back (see `askOperatorForRefund` in `bootloader.yul` for details).
        if let Some(bootloader_refund) = self.requested_refund() {
            assert!(
                self.operator_refund.is_none(),
                "Operator was asked for refund two times"
            );

            let current_tx_index = bootloader_state.current_tx();
            let tx_description_offset =
                bootloader_state.get_tx_description_offset(current_tx_index);
            let tx_gas_limit = state
                .memory
                .read_slot(
                    BOOTLOADER_HEAP_PAGE as usize,
                    tx_description_offset + TX_GAS_LIMIT_OFFSET,
                )
                .value
                .as_u64();

            assert!(
                state.local_state.pubdata_revert_counter.0 >= 0,
                "Global counter is negative"
            );
            let current_counter = state.local_state.pubdata_revert_counter.0 as u32;

            self.pubdata_published =
                current_counter.saturating_sub(self.spent_pubdata_counter_before);

            let tx_body_refund = self.tx_body_refund(
                bootloader_refund.refund,
                bootloader_refund.gas_spent_on_pubdata,
                tx_gas_limit,
                bootloader_refund.used_gas_per_pubdata_byte,
                self.pubdata_published,
                bootloader_state.last_l2_block().txs.last().unwrap().hash,
            );

            if tx_body_refund < bootloader_refund.refund {
                tracing::error!(
                    "Suggested tx body refund is less than bootloader refund. Tx body refund: {}, \
                     bootloader refund: {}",
                    tx_body_refund,
                    bootloader_refund.refund
                );
            }

            let refund_to_propose = tx_body_refund + self.block_overhead_refund();

            let refund_slot = OPERATOR_REFUNDS_OFFSET + current_tx_index;

            // Writing the refund into memory
            state.memory.populate_page(
                BOOTLOADER_HEAP_PAGE as usize,
                vec![(refund_slot, refund_to_propose.into())],
                self.timestamp_before_cycle,
            );

            bootloader_state.set_refund_for_current_tx(refund_to_propose);
            self.operator_refund = Some(refund_to_propose);
            self.set_refund_as_done();

            if tx_gas_limit < bootloader_refund.refund {
                tracing::error!(
                    "Tx gas limit is less than bootloader refund. Tx gas limit: {}, \
                    bootloader refund: {}",
                    tx_gas_limit,
                    bootloader_refund.refund
                );
            }
            if tx_gas_limit < refund_to_propose {
                tracing::error!(
                    "Tx gas limit is less than operator refund. Tx gas limit: {tx_gas_limit}, \
                     operator refund: {refund_to_propose}"
                );
            }

            METRICS.refund[&RefundType::Bootloader]
                .observe(bootloader_refund.refund as f64 / tx_gas_limit as f64 * 100.0);
            METRICS.refund[&RefundType::Operator]
                .observe(refund_to_propose as f64 / tx_gas_limit as f64 * 100.0);
            let refund_diff = (refund_to_propose as f64 - bootloader_refund.refund as f64)
                / tx_gas_limit as f64
                * 100.0;
            METRICS.refund_diff.observe(refund_diff);
        }
        TracerExecutionStatus::Continue
    }
}
