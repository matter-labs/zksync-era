use std::{
    cmp::{max, min},
    fmt,
    sync::Arc,
};

use zksync_eth_client::{ClientError, EnrichedClientError};
use zksync_node_fee_model::l1_gas_price::TxParamsProvider;
use zksync_types::eth_sender::TxHistory;

use crate::{abstract_l1_interface::OperatorType, EthSenderError};

#[derive(Debug)]
pub(crate) struct EthFees {
    pub(crate) base_fee_per_gas: u64,
    pub(crate) priority_fee_per_gas: u64,
    pub(crate) blob_base_fee_per_gas: Option<u64>,
    pub(crate) max_gas_per_pubdata_price: Option<u64>,
}

pub(crate) trait EthFeesOracle: 'static + Sync + Send + fmt::Debug {
    fn calculate_fees(
        &self,
        previous_sent_tx: &Option<TxHistory>,
        time_in_mempool_in_l1_blocks: u32,
        operator_type: OperatorType,
    ) -> Result<EthFees, EthSenderError>;
}

#[derive(Debug)]
pub(crate) struct GasAdjusterFeesOracle {
    pub gas_adjuster: Arc<dyn TxParamsProvider>,
    pub max_acceptable_priority_fee_in_gwei: u64,
    pub time_in_mempool_in_l1_blocks_cap: u32,
    pub max_acceptable_base_fee_in_wei: u64,
}

impl GasAdjusterFeesOracle {
    fn assert_fee_is_not_zero(&self, value: u64, fee_type: &'static str) {
        if value == 0 {
            panic!(
                "L1 RPC incorrectly reported {fee_type} prices, either it doesn't return them at \
            all or returns 0's, eth-sender cannot continue without proper {fee_type} prices!"
            );
        }
    }

    fn is_base_fee_exceeding_limit(&self, value: u64) -> bool {
        if value > self.max_acceptable_base_fee_in_wei {
            tracing::warn!(
                    "base fee per gas: {} exceed max acceptable fee in configuration: {}, skip transaction",
                    value,
                    self.max_acceptable_base_fee_in_wei
            );
            return true;
        }
        false
    }

    fn calculate_fees_with_blob_sidecar(
        &self,
        previous_sent_tx: &Option<TxHistory>,
        time_in_mempool_in_l1_blocks: u32,
    ) -> Result<EthFees, EthSenderError> {
        const MIN_PRICE_BUMP_MULTIPLIER: f64 = 2.00;

        // we cap it to not allow nearly infinite values when a tx is stuck for a long time
        let capped_time_in_mempool_in_l1_blocks = min(
            time_in_mempool_in_l1_blocks,
            self.time_in_mempool_in_l1_blocks_cap,
        );

        let base_fee_per_gas = self
            .gas_adjuster
            .get_blob_tx_base_fee(capped_time_in_mempool_in_l1_blocks);
        self.assert_fee_is_not_zero(base_fee_per_gas, "base");
        if self.is_base_fee_exceeding_limit(base_fee_per_gas) {
            return Err(EthSenderError::ExceedMaxBaseFee);
        }
        let blob_base_fee_per_gas = self
            .gas_adjuster
            .get_blob_tx_blob_base_fee(capped_time_in_mempool_in_l1_blocks);
        self.assert_fee_is_not_zero(blob_base_fee_per_gas, "blob");

        let mut priority_fee_per_gas = self.gas_adjuster.get_blob_tx_priority_fee();
        if let Some(previous_sent_tx) = previous_sent_tx {
            self.verify_base_fee_not_too_low_on_resend(
                previous_sent_tx.id,
                previous_sent_tx.blob_base_fee_per_gas.unwrap_or(0),
                blob_base_fee_per_gas,
                self.gas_adjuster.get_next_block_minimal_blob_base_fee(),
                MIN_PRICE_BUMP_MULTIPLIER,
                "blob_base_fee_per_gas",
            )?;

            self.verify_base_fee_not_too_low_on_resend(
                previous_sent_tx.id,
                previous_sent_tx.base_fee_per_gas,
                base_fee_per_gas,
                self.gas_adjuster.get_next_block_minimal_base_fee(),
                MIN_PRICE_BUMP_MULTIPLIER,
                "base_fee_per_gas",
            )?;

            priority_fee_per_gas = max(
                priority_fee_per_gas,
                (previous_sent_tx.priority_fee_per_gas as f64 * MIN_PRICE_BUMP_MULTIPLIER).ceil()
                    as u64,
            );
        }
        Ok(EthFees {
            base_fee_per_gas,
            priority_fee_per_gas,
            blob_base_fee_per_gas: Some(blob_base_fee_per_gas),
            max_gas_per_pubdata_price: None,
        })
    }

    fn calculate_fees_no_blob_sidecar(
        &self,
        previous_sent_tx: &Option<TxHistory>,
        time_in_mempool_in_l1_blocks: u32,
    ) -> Result<EthFees, EthSenderError> {
        const MIN_PRICE_BUMP_MULTIPLIER: f64 = 1.10;

        // we cap it to not allow nearly infinite values when a tx is stuck for a long time
        let capped_time_in_mempool_in_l1_blocks = min(
            time_in_mempool_in_l1_blocks,
            self.time_in_mempool_in_l1_blocks_cap,
        );
        let base_fee_per_gas = self
            .gas_adjuster
            .get_base_fee(capped_time_in_mempool_in_l1_blocks);
        self.assert_fee_is_not_zero(base_fee_per_gas, "base");
        if self.is_base_fee_exceeding_limit(base_fee_per_gas) {
            return Err(EthSenderError::ExceedMaxBaseFee);
        }

        let mut priority_fee_per_gas = self.gas_adjuster.get_priority_fee();

        if let Some(previous_sent_tx) = previous_sent_tx {
            self.verify_base_fee_not_too_low_on_resend(
                previous_sent_tx.id,
                previous_sent_tx.base_fee_per_gas,
                base_fee_per_gas,
                self.gas_adjuster.get_next_block_minimal_base_fee(),
                MIN_PRICE_BUMP_MULTIPLIER,
                "base_fee_per_gas",
            )?;

            priority_fee_per_gas = max(
                priority_fee_per_gas,
                (previous_sent_tx.priority_fee_per_gas as f64 * MIN_PRICE_BUMP_MULTIPLIER).ceil()
                    as u64,
            );
        }

        // Extra check to prevent sending transaction will extremely high priority fee.
        if priority_fee_per_gas > self.max_acceptable_priority_fee_in_gwei {
            panic!(
                "Extremely high value of priority_fee_per_gas is suggested: {}, while max acceptable is {}",
                priority_fee_per_gas,
                self.max_acceptable_priority_fee_in_gwei
            );
        }

        Ok(EthFees {
            base_fee_per_gas,
            blob_base_fee_per_gas: None,
            priority_fee_per_gas,
            max_gas_per_pubdata_price: None,
        })
    }

    fn calculate_fees_for_gateway_tx(
        &self,
        previous_sent_tx: &Option<TxHistory>,
        time_in_mempool_in_l1_blocks: u32,
    ) -> Result<EthFees, EthSenderError> {
        const MIN_PRICE_BUMP_MULTIPLIER: f64 = 1.10;

        // we cap it to not allow nearly infinite values when a tx is stuck for a long time
        let capped_time_in_mempool_in_l1_blocks = min(
            time_in_mempool_in_l1_blocks,
            self.time_in_mempool_in_l1_blocks_cap,
        );
        let base_fee_per_gas = self
            .gas_adjuster
            .get_base_fee(capped_time_in_mempool_in_l1_blocks);
        self.assert_fee_is_not_zero(base_fee_per_gas, "base");
        if self.is_base_fee_exceeding_limit(base_fee_per_gas) {
            return Err(EthSenderError::ExceedMaxBaseFee);
        }

        let mut gas_per_pubdata = self
            .gas_adjuster
            .get_gateway_price_per_pubdata(capped_time_in_mempool_in_l1_blocks);

        if let Some(previous_sent_tx) = previous_sent_tx {
            self.verify_base_fee_not_too_low_on_resend(
                previous_sent_tx.id,
                previous_sent_tx.base_fee_per_gas,
                base_fee_per_gas,
                self.gas_adjuster.get_next_block_minimal_base_fee(),
                MIN_PRICE_BUMP_MULTIPLIER,
                "base_fee_per_gas",
            )?;

            // Increase `gas_per_pubdata_fee`.
            gas_per_pubdata =
                if let Some(prev_gas_per_pubdata) = previous_sent_tx.max_gas_per_pubdata {
                    max(
                        gas_per_pubdata,
                        (prev_gas_per_pubdata as f64 * MIN_PRICE_BUMP_MULTIPLIER).ceil() as u64,
                    )
                } else {
                    gas_per_pubdata
                };
        }

        Ok(EthFees {
            base_fee_per_gas,
            blob_base_fee_per_gas: None,
            // We ignore priority fee for gateway
            priority_fee_per_gas: 0,
            max_gas_per_pubdata_price: Some(gas_per_pubdata),
        })
    }

    fn verify_base_fee_not_too_low_on_resend(
        &self,
        tx_id: u32,
        previous_fee: u64,
        fee_to_use: u64,
        next_block_minimal_fee: u64,
        min_price_bump_multiplier: f64,
        fee_type: &str,
    ) -> Result<(), EthSenderError> {
        let fee_to_use = fee_to_use as f64;
        if fee_to_use < (next_block_minimal_fee as f64)
            || fee_to_use < (previous_fee as f64 * min_price_bump_multiplier).ceil()
        {
            // If the fee is lower than the previous used one multiplied by the required factor
            // or is lower than the minimal possible value for the next block, sending is skipped.
            tracing::info!(
                "{fee_type} too low for resend detected for tx {}, \
                 suggested fee {:?}, \
                 previous_fee {:?}, \
                 next_block_minimal_fee {:?}, \
                 min_price_bump_multiplier {:?}",
                tx_id,
                fee_to_use,
                previous_fee,
                next_block_minimal_fee,
                min_price_bump_multiplier
            );
            let err = ClientError::Custom(format!("{fee_type} is too low"));
            let err = EnrichedClientError::new(err, "verify_base_fee_not_too_low_on_resend")
                .with_arg("fee_to_use", &fee_to_use)
                .with_arg("previous_fee", &previous_fee)
                .with_arg("next_block_minimal_fee", &next_block_minimal_fee)
                .with_arg("min_price_bump_multiplier", &min_price_bump_multiplier);
            return Err(err.into());
        }
        Ok(())
    }
}

impl EthFeesOracle for GasAdjusterFeesOracle {
    fn calculate_fees(
        &self,
        previous_sent_tx: &Option<TxHistory>,
        time_in_mempool_in_l1_blocks: u32,
        operator_type: OperatorType,
    ) -> Result<EthFees, EthSenderError> {
        match operator_type {
            OperatorType::NonBlob => {
                self.calculate_fees_no_blob_sidecar(previous_sent_tx, time_in_mempool_in_l1_blocks)
            }
            OperatorType::Blob => self
                .calculate_fees_with_blob_sidecar(previous_sent_tx, time_in_mempool_in_l1_blocks),
            OperatorType::Gateway => {
                self.calculate_fees_for_gateway_tx(previous_sent_tx, time_in_mempool_in_l1_blocks)
            }
            OperatorType::Tee => {
                // FIXME: TEE
                self.calculate_fees_no_blob_sidecar(previous_sent_tx, time_in_mempool_in_l1_blocks)
            }
        }
    }
}
