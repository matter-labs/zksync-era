use crate::vm_1_3_2::history_recorder::HistoryMode;
use crate::vm_1_3_2::vm_with_bootloader::{
    eth_price_per_pubdata_byte, BOOTLOADER_HEAP_PAGE, TX_GAS_LIMIT_OFFSET,
};
use crate::vm_1_3_2::VmInstance;
use zk_evm_1_3_3::aux_structures::Timestamp;
use zksync_state::WriteStorage;
use zksync_types::U256;
use zksync_utils::ceil_div_u256;

impl<H: HistoryMode, S: WriteStorage> VmInstance<S, H> {
    pub(crate) fn tx_body_refund(
        &self,
        from_timestamp: Timestamp,
        bootloader_refund: u32,
        gas_spent_on_pubdata: u32,
    ) -> u32 {
        let current_tx_index = self.bootloader_state.tx_to_execute() - 1;
        let tx_gas_limit = self.get_tx_gas_limit(current_tx_index);
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

        let pubdata_published = self.pubdata_published(from_timestamp);

        // For now, bootloader charges only for base fee.
        let effective_gas_price = self.block_context.base_fee;

        let bootloader_eth_price_per_pubdata_byte = U256::from(effective_gas_price)
            * U256::from(self.state.local_state.current_ergs_per_pubdata_byte);
        let fair_eth_price_per_pubdata_byte = U256::from(eth_price_per_pubdata_byte(
            self.block_context.context.l1_gas_price,
        ));

        // For now, L1 originated transactions are allowed to pay less than fair fee per pubdata,
        // so we should take it into account.
        let eth_price_per_pubdata_byte_for_calculation = std::cmp::min(
            bootloader_eth_price_per_pubdata_byte,
            fair_eth_price_per_pubdata_byte,
        );

        let fair_fee_eth = U256::from(gas_spent_on_computation)
            * U256::from(self.block_context.context.fair_l2_gas_price)
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

    /// Calculates the refund for the block overhead.
    /// This refund is the difference between how much user paid in advance for the block overhead
    /// and how much he should pay based on actual tx execution result.
    pub(crate) fn block_overhead_refund(
        &self,
        _from_timestamp: Timestamp,
        _gas_remaining_before: u32,
        _gas_spent_on_pubdata: u32,
    ) -> u32 {
        // TODO (SMA-1715): Make users pay for the block overhead
        0

        // let pubdata_published = self.pubdata_published(from_timestamp);
        //
        // let total_gas_spent = gas_remaining_before - self.gas_remaining();
        // let gas_spent_on_computation = total_gas_spent.checked_sub(gas_spent_on_pubdata).unwrap_or_else(|| {
        //     tracing::error!("Gas spent on pubdata is greater than total gas spent. On pubdata: {}, total: {}", gas_spent_on_pubdata, total_gas_spent);
        //     0
        // });
        // let (_, l2_to_l1_logs) = self.collect_events_and_l1_logs_after_timestamp(from_timestamp);
        // let current_tx_index = self.bootloader_state.tx_to_execute() - 1;
        //
        // let actual_overhead = Self::actual_overhead_gas(
        //     self.state.local_state.current_ergs_per_pubdata_byte,
        //     self.bootloader_state.get_tx_size(current_tx_index),
        //     pubdata_published,
        //     gas_spent_on_computation,
        //     self.state
        //         .decommittment_processor
        //         .get_number_of_decommitment_requests_after_timestamp(from_timestamp),
        //     l2_to_l1_logs.len(),
        // );
        //
        // let predefined_overhead = self
        //     .state
        //     .memory
        //     .read_slot(
        //         BOOTLOADER_HEAP_PAGE as usize,
        //         TX_OVERHEAD_OFFSET + current_tx_index,
        //     )
        //     .value
        //     .as_u32();
        //
        // if actual_overhead <= predefined_overhead {
        //     predefined_overhead - actual_overhead
        // } else {
        //     // This should never happen but potential mistakes at the early stage should not bring the server down.
        //     // TODO (SMA-1700): log addition information (e.g overhead for each of criteria: pubdata, single-instance circuits capacity etc)
        //     //   to make debugging easier.
        //     tracing::error!(
        //         "Actual overhead is greater than predefined one, actual: {}, predefined: {}",
        //         actual_overhead,
        //         predefined_overhead
        //     );
        //     0
        // }
    }

    // TODO (SMA-1715): Make users pay for the block overhead
    #[allow(dead_code)]
    fn actual_overhead_gas(
        _gas_per_pubdata_byte_limit: u32,
        _encoded_len: usize,
        _pubdata_published: u32,
        _gas_spent_on_computation: u32,
        _number_of_decommitment_requests: usize,
        _l2_l1_logs: usize,
    ) -> u32 {
        0

        // let overhead_for_block_gas = U256::from(crate::transaction_data::block_overhead_gas(
        //     gas_per_pubdata_byte_limit,
        // ));

        // let encoded_len = U256::from(encoded_len);
        // let pubdata_published = U256::from(pubdata_published);
        // let gas_spent_on_computation = U256::from(gas_spent_on_computation);
        // let number_of_decommitment_requests = U256::from(number_of_decommitment_requests);
        // let l2_l1_logs = U256::from(l2_l1_logs);

        // let tx_slot_overhead = ceil_div_u256(overhead_for_block_gas, MAX_TXS_IN_BLOCK.into());

        // let overhead_for_length = ceil_div_u256(
        //     encoded_len * overhead_for_block_gas,
        //     BOOTLOADER_TX_ENCODING_SPACE.into(),
        // );

        // let actual_overhead_for_pubdata = ceil_div_u256(
        //     pubdata_published * overhead_for_block_gas,
        //     MAX_PUBDATA_PER_BLOCK.into(),
        // );

        // let actual_gas_limit_overhead = ceil_div_u256(
        //     gas_spent_on_computation * overhead_for_block_gas,
        //     MAX_BLOCK_MULTIINSTANCE_GAS_LIMIT.into(),
        // );

        // let code_decommitter_sorter_circuit_overhead = ceil_div_u256(
        //     number_of_decommitment_requests * overhead_for_block_gas,
        //     GEOMETRY_CONFIG.limit_for_code_decommitter_sorter.into(),
        // );

        // let l1_l2_logs_overhead = ceil_div_u256(
        //     l2_l1_logs * overhead_for_block_gas,
        //     std::cmp::min(
        //         GEOMETRY_CONFIG.limit_for_l1_messages_merklizer,
        //         GEOMETRY_CONFIG.limit_for_l1_messages_pudata_hasher,
        //     )
        //     .into(),
        // );

        // let overhead = vec![
        //     tx_slot_overhead,
        //     overhead_for_length,
        //     actual_overhead_for_pubdata,
        //     actual_gas_limit_overhead,
        //     code_decommitter_sorter_circuit_overhead,
        //     l1_l2_logs_overhead,
        // ]
        // .into_iter()
        // .max()
        // .unwrap();

        // overhead.as_u32()
    }

    /// Returns the given transactions' gas limit - by reading it directly from the VM memory.
    pub(crate) fn get_tx_gas_limit(&self, tx_index: usize) -> u32 {
        let tx_description_offset = self.bootloader_state.get_tx_description_offset(tx_index);
        self.state
            .memory
            .read_slot(
                BOOTLOADER_HEAP_PAGE as usize,
                tx_description_offset + TX_GAS_LIMIT_OFFSET,
            )
            .value
            .as_u32()
    }
}
