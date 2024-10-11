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
    #[allow(dead_code)]
    pub(crate) pubdata_price: Option<u64>,
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
    fn calculate_fees_with_blob_sidecar(
        &self,
        previous_sent_tx: &Option<TxHistory>,
    ) -> Result<EthFees, EthSenderError> {
        let base_fee_per_gas = self.gas_adjuster.get_blob_tx_base_fee();
        self.assert_fee_is_not_zero(base_fee_per_gas, "base");
        let priority_fee_per_gas = self.gas_adjuster.get_blob_tx_priority_fee();
        let blob_base_fee_per_gas = self.gas_adjuster.get_blob_tx_blob_base_fee();
        self.assert_fee_is_not_zero(blob_base_fee_per_gas, "blob");
        let blob_base_fee_per_gas = Some(blob_base_fee_per_gas);

        if let Some(previous_sent_tx) = previous_sent_tx {
            // for blob transactions on re-sending need to double all gas prices
            return Ok(EthFees {
                base_fee_per_gas: max(previous_sent_tx.base_fee_per_gas * 2, base_fee_per_gas),
                priority_fee_per_gas: max(
                    previous_sent_tx.priority_fee_per_gas * 2,
                    priority_fee_per_gas,
                ),
                blob_base_fee_per_gas: max(
                    previous_sent_tx.blob_base_fee_per_gas.map(|v| v * 2),
                    blob_base_fee_per_gas,
                ),
                pubdata_price: None,
            });
        }
        Ok(EthFees {
            base_fee_per_gas,
            priority_fee_per_gas,
            blob_base_fee_per_gas,
            pubdata_price: None,
        })
    }

    fn calculate_fees_no_blob_sidecar(
        &self,
        previous_sent_tx: &Option<TxHistory>,
        time_in_mempool_in_l1_blocks: u32,
    ) -> Result<EthFees, EthSenderError> {
        // we cap it to not allow nearly infinite values when a tx is stuck for a long time
        let capped_time_in_mempool_in_l1_blocks = min(
            time_in_mempool_in_l1_blocks,
            self.time_in_mempool_in_l1_blocks_cap,
        );
        let mut base_fee_per_gas = self
            .gas_adjuster
            .get_base_fee(capped_time_in_mempool_in_l1_blocks);
        self.assert_fee_is_not_zero(base_fee_per_gas, "base");
        if let Some(previous_sent_tx) = previous_sent_tx {
            self.verify_base_fee_not_too_low_on_resend(
                previous_sent_tx.id,
                previous_sent_tx.base_fee_per_gas,
                base_fee_per_gas,
            )?;
        }

        let mut priority_fee_per_gas = self.gas_adjuster.get_priority_fee();

        if let Some(previous_sent_tx) = previous_sent_tx {
            // Increase `priority_fee_per_gas` by at least 20% to prevent "replacement transaction under-priced" error.
            priority_fee_per_gas = max(
                priority_fee_per_gas,
                (previous_sent_tx.priority_fee_per_gas * 6) / 5 + 1,
            );

            // same for base_fee_per_gas but 10%
            base_fee_per_gas = max(
                base_fee_per_gas,
                previous_sent_tx.base_fee_per_gas + (previous_sent_tx.base_fee_per_gas / 10) + 1,
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
            pubdata_price: None,
        })
    }

    fn verify_base_fee_not_too_low_on_resend(
        &self,
        tx_id: u32,
        previous_base_fee: u64,
        base_fee_to_use: u64,
    ) -> Result<(), EthSenderError> {
        let next_block_minimal_base_fee = self.gas_adjuster.get_next_block_minimal_base_fee();
        if base_fee_to_use < min(next_block_minimal_base_fee, previous_base_fee) {
            // If the base fee is lower than the previous used one
            // or is lower than the minimal possible value for the next block, sending is skipped.
            tracing::info!(
                "Base fee too low for resend detected for tx {}, \
                 suggested base_fee_per_gas {:?}, \
                 previous_base_fee {:?}, \
                 next_block_minimal_base_fee {:?}",
                tx_id,
                base_fee_to_use,
                previous_base_fee,
                next_block_minimal_base_fee
            );
            let err = ClientError::Custom("base_fee_per_gas is too low".into());
            let err = EnrichedClientError::new(err, "increase_priority_fee")
                .with_arg("base_fee_to_use", &base_fee_to_use)
                .with_arg("previous_base_fee", &previous_base_fee)
                .with_arg("next_block_minimal_base_fee", &next_block_minimal_base_fee);
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
        let has_blob_sidecar = operator_type == OperatorType::Blob;
        if has_blob_sidecar {
            self.calculate_fees_with_blob_sidecar(previous_sent_tx)
        } else {
            self.calculate_fees_no_blob_sidecar(previous_sent_tx, time_in_mempool_in_l1_blocks)
        }
    }
}
