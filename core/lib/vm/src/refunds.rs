use crate::history_recorder::HistoryMode;
use crate::vm_with_bootloader::{
    eth_price_per_pubdata_byte, BOOTLOADER_HEAP_PAGE, TX_GAS_LIMIT_OFFSET,
};
use crate::VmInstance;
use zk_evm::aux_structures::Timestamp;
use zksync_types::U256;
use zksync_utils::ceil_div_u256;

impl<H: HistoryMode> VmInstance<'_, H> {
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
                vlog::error!(
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
            vlog::error!(
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
        0
    }

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
