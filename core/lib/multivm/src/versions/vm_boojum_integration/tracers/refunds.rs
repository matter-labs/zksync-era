use std::marker::PhantomData;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};
use zk_evm_1_4_0::{
    aux_structures::Timestamp,
    tracing::{BeforeExecutionData, VmLocalStateData},
    vm_state::VmLocalState,
};
use zksync_system_constants::{PUBLISH_BYTECODE_OVERHEAD, SYSTEM_CONTEXT_ADDRESS};
use zksync_types::{l2_to_l1_log::L2ToL1Log, L1BatchNumber, U256};
use zksync_utils::{bytecode::bytecode_len_in_bytes, ceil_div_u256, u256_to_h256};

use crate::{
    interface::{
        storage::{StoragePtr, WriteStorage},
        tracer::TracerExecutionStatus,
        L1BatchEnv, Refunds, VmEvent,
    },
    tracers::dynamic::vm_1_4_0::DynTracer,
    vm_boojum_integration::{
        bootloader_state::BootloaderState,
        constants::{BOOTLOADER_HEAP_PAGE, OPERATOR_REFUNDS_OFFSET, TX_GAS_LIMIT_OFFSET},
        old_vm::{
            events::merge_events, history_recorder::HistoryMode, memory::SimpleMemory,
            utils::eth_price_per_pubdata_byte,
        },
        tracers::{
            traits::VmTracer,
            utils::{
                gas_spent_on_bytecodes_and_long_messages_this_opcode, get_vm_hook_params, VmHook,
            },
        },
        types::internals::ZkSyncVmState,
        utils::fee::get_batch_base_fee,
    },
};

/// Tracer responsible for collecting information about refunds.
#[derive(Debug, Clone)]
pub(crate) struct RefundsTracer<S> {
    // Some(x) means that the bootloader has asked the operator
    // to provide the refund the user, where `x` is the refund proposed
    // by the bootloader itself.
    pending_operator_refund: Option<u32>,
    refund_gas: u32,
    operator_refund: Option<u32>,
    timestamp_initial: Timestamp,
    timestamp_before_cycle: Timestamp,
    gas_remaining_before: u32,
    spent_pubdata_counter_before: u32,
    gas_spent_on_bytecodes_and_long_messages: u32,
    l1_batch: L1BatchEnv,
    pubdata_published: u32,
    _phantom: PhantomData<S>,
}

impl<S> RefundsTracer<S> {
    pub(crate) fn new(l1_batch: L1BatchEnv) -> Self {
        Self {
            pending_operator_refund: None,
            refund_gas: 0,
            operator_refund: None,
            timestamp_initial: Timestamp(0),
            timestamp_before_cycle: Timestamp(0),
            gas_remaining_before: 0,
            spent_pubdata_counter_before: 0,
            gas_spent_on_bytecodes_and_long_messages: 0,
            l1_batch,
            pubdata_published: 0,
            _phantom: PhantomData,
        }
    }
}

impl<S> RefundsTracer<S> {
    fn requested_refund(&self) -> Option<u32> {
        self.pending_operator_refund
    }

    fn set_refund_as_done(&mut self) {
        self.pending_operator_refund = None;
    }

    fn block_overhead_refund(&mut self) -> u32 {
        0
    }

    pub(crate) fn get_refunds(&self) -> Refunds {
        Refunds {
            gas_refunded: self.refund_gas as u64,
            operator_suggested_refund: self.operator_refund.unwrap_or_default() as u64,
        }
    }

    pub(crate) fn tx_body_refund(
        &self,
        bootloader_refund: u32,
        gas_spent_on_pubdata: u32,
        tx_gas_limit: u32,
        current_ergs_per_pubdata_byte: u32,
        pubdata_published: u32,
    ) -> u32 {
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

        let fair_eth_price_per_pubdata_byte = U256::from(eth_price_per_pubdata_byte(
            self.l1_batch.fee_input.l1_gas_price(),
        ));

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

        ceil_div_u256(refund_eth, effective_gas_price.into()).as_u32()
    }

    pub(crate) fn gas_spent_on_pubdata(&self, vm_local_state: &VmLocalState) -> u32 {
        self.gas_spent_on_bytecodes_and_long_messages + vm_local_state.spent_pubdata_counter
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
        let hook = VmHook::from_opcode_memory(&state, &data);
        match hook {
            VmHook::NotifyAboutRefund => self.refund_gas = get_vm_hook_params(memory)[0].as_u32(),
            VmHook::AskOperatorForRefund => {
                self.pending_operator_refund = Some(get_vm_hook_params(memory)[0].as_u32())
            }
            _ => {}
        }

        self.gas_spent_on_bytecodes_and_long_messages +=
            gas_spent_on_bytecodes_and_long_messages_this_opcode(&state, &data);
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for RefundsTracer<S> {
    fn initialize_tracer(&mut self, state: &mut ZkSyncVmState<S, H>) {
        self.timestamp_initial = Timestamp(state.local_state.timestamp);
        self.gas_remaining_before = state.local_state.callstack.current.ergs_remaining;
        self.spent_pubdata_counter_before = state.local_state.spent_pubdata_counter;
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
        #[metrics(prefix = "vm_boojum_integration")]
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
            let gas_spent_on_pubdata =
                self.gas_spent_on_pubdata(&state.local_state) - self.spent_pubdata_counter_before;

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
                .as_u32();

            let used_published_storage_slots = state
                .storage
                .save_paid_changes(Timestamp(state.local_state.timestamp));

            let pubdata_published = pubdata_published(
                state,
                used_published_storage_slots,
                self.timestamp_initial,
                self.l1_batch.number,
            );

            self.pubdata_published = pubdata_published;
            let current_ergs_per_pubdata_byte = state.local_state.current_ergs_per_pubdata_byte;
            let tx_body_refund = self.tx_body_refund(
                bootloader_refund,
                gas_spent_on_pubdata,
                tx_gas_limit,
                current_ergs_per_pubdata_byte,
                pubdata_published,
            );

            if tx_body_refund < bootloader_refund {
                tracing::error!(
                    "Suggested tx body refund is less than bootloader refund. Tx body refund: {tx_body_refund}, \
                     bootloader refund: {bootloader_refund}"
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

            if tx_gas_limit < bootloader_refund {
                tracing::error!(
                    "Tx gas limit is less than bootloader refund. Tx gas limit: {tx_gas_limit}, \
                    bootloader refund: {bootloader_refund}"
                );
            }
            if tx_gas_limit < refund_to_propose {
                tracing::error!(
                    "Tx gas limit is less than operator refund. Tx gas limit: {tx_gas_limit}, \
                     operator refund: {refund_to_propose}"
                );
            }

            METRICS.refund[&RefundType::Bootloader]
                .observe(bootloader_refund as f64 / tx_gas_limit as f64 * 100.0);
            METRICS.refund[&RefundType::Operator]
                .observe(refund_to_propose as f64 / tx_gas_limit as f64 * 100.0);
            let refund_diff =
                (refund_to_propose as f64 - bootloader_refund as f64) / tx_gas_limit as f64 * 100.0;
            METRICS.refund_diff.observe(refund_diff);
        }
        TracerExecutionStatus::Continue
    }
}

/// Returns the given transactions' gas limit - by reading it directly from the VM memory.
pub(crate) fn pubdata_published<S: WriteStorage, H: HistoryMode>(
    state: &ZkSyncVmState<S, H>,
    storage_writes_pubdata_published: u32,
    from_timestamp: Timestamp,
    batch_number: L1BatchNumber,
) -> u32 {
    let (raw_events, l1_messages) = state
        .event_sink
        .get_events_and_l2_l1_logs_after_timestamp(from_timestamp);
    let events: Vec<_> = merge_events(raw_events)
        .into_iter()
        .map(|e| e.into_vm_event(batch_number))
        .collect();
    // For the first transaction in L1 batch there may be (it depends on the execution mode) an L2->L1 log
    // that is sent by `SystemContext` in `setNewBlock`. It's a part of the L1 batch pubdata overhead and not the transaction itself.
    let l2_l1_logs_bytes = (l1_messages
        .into_iter()
        .map(|log| L2ToL1Log {
            shard_id: log.shard_id,
            is_service: log.is_first,
            tx_number_in_block: log.tx_number_in_block,
            sender: log.address,
            key: u256_to_h256(log.key),
            value: u256_to_h256(log.value),
        })
        .filter(|log| log.sender != SYSTEM_CONTEXT_ADDRESS)
        .count() as u32)
        * zk_evm_1_4_0::zkevm_opcode_defs::system_params::L1_MESSAGE_PUBDATA_BYTES;
    let l2_l1_long_messages_bytes: u32 = VmEvent::extract_long_l2_to_l1_messages(&events)
        .iter()
        .map(|event| event.len() as u32)
        .sum();

    let published_bytecode_bytes: u32 = VmEvent::extract_published_bytecodes(&events)
        .iter()
        .map(|bytecodehash| bytecode_len_in_bytes(*bytecodehash) as u32 + PUBLISH_BYTECODE_OVERHEAD)
        .sum();

    storage_writes_pubdata_published
        + l2_l1_logs_bytes
        + l2_l1_long_messages_bytes
        + published_bytecode_bytes
}
