use anyhow::Context as _;
use zksync_config::configs::{self};
use zksync_protobuf::{required, ProtoRepr};

use crate::{proto::eth as proto, read_optional_repr};

impl proto::ProofSendingMode {
    fn new(x: &configs::eth_sender::ProofSendingMode) -> Self {
        use configs::eth_sender::ProofSendingMode as From;
        match x {
            From::OnlyRealProofs => Self::OnlyRealProofs,
            From::OnlySampledProofs => Self::OnlySampledProofs,
            From::SkipEveryProof => Self::SkipEveryProof,
        }
    }

    fn parse(&self) -> configs::eth_sender::ProofSendingMode {
        use configs::eth_sender::ProofSendingMode as To;
        match self {
            Self::OnlyRealProofs => To::OnlyRealProofs,
            Self::OnlySampledProofs => To::OnlySampledProofs,
            Self::SkipEveryProof => To::SkipEveryProof,
        }
    }
}

impl proto::PubdataSendingMode {
    fn new(x: &configs::eth_sender::PubdataSendingMode) -> Self {
        use configs::eth_sender::PubdataSendingMode as From;
        match x {
            From::Calldata => Self::Calldata,
            From::Blobs => Self::Blobs,
            From::Custom => Self::Custom,
        }
    }

    fn parse(&self) -> configs::eth_sender::PubdataSendingMode {
        use configs::eth_sender::PubdataSendingMode as To;
        match self {
            Self::Calldata => To::Calldata,
            Self::Blobs => To::Blobs,
            Self::Custom => To::Custom,
        }
    }
}

impl ProtoRepr for proto::Eth {
    type Type = configs::eth_sender::EthConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            sender: read_optional_repr(&self.sender).context("sender")?,
            gas_adjuster: read_optional_repr(&self.gas_adjuster).context("gas_adjuster")?,
            watcher: read_optional_repr(&self.watcher).context("watcher")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            sender: this.sender.as_ref().map(ProtoRepr::build),
            gas_adjuster: this.gas_adjuster.as_ref().map(ProtoRepr::build),
            watcher: this.watcher.as_ref().map(ProtoRepr::build),
        }
    }
}

impl ProtoRepr for proto::Sender {
    type Type = configs::eth_sender::SenderConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            aggregated_proof_sizes: self
                .aggregated_proof_sizes
                .iter()
                .enumerate()
                .map(|(i, x)| (*x).try_into().context(i))
                .collect::<Result<_, _>>()
                .context("aggregated_proof_sizes")?,
            wait_confirmations: self.wait_confirmations,
            tx_poll_period: *required(&self.tx_poll_period).context("tx_poll_period")?,
            aggregate_tx_poll_period: *required(&self.aggregate_tx_poll_period)
                .context("aggregate_tx_poll_period")?,
            max_txs_in_flight: *required(&self.max_txs_in_flight).context("max_txs_in_flight")?,
            proof_sending_mode: required(&self.proof_sending_mode)
                .and_then(|x| Ok(proto::ProofSendingMode::try_from(*x)?))
                .context("proof_sending_mode")?
                .parse(),
            max_aggregated_tx_gas: *required(&self.max_aggregated_tx_gas)
                .context("max_aggregated_tx_gas")?,
            max_eth_tx_data_size: required(&self.max_eth_tx_data_size)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_eth_tx_data_size")?,
            max_aggregated_blocks_to_commit: *required(&self.max_aggregated_blocks_to_commit)
                .context("max_aggregated_blocks_to_commit")?,
            max_aggregated_blocks_to_execute: *required(&self.max_aggregated_blocks_to_execute)
                .context("max_aggregated_blocks_to_execute")?,
            aggregated_block_commit_deadline: *required(&self.aggregated_block_commit_deadline)
                .context("aggregated_block_commit_deadline")?,
            aggregated_block_prove_deadline: *required(&self.aggregated_block_prove_deadline)
                .context("aggregated_block_prove_deadline")?,
            aggregated_block_execute_deadline: *required(&self.aggregated_block_execute_deadline)
                .context("aggregated_block_execute_deadline")?,
            timestamp_criteria_max_allowed_lag: required(&self.timestamp_criteria_max_allowed_lag)
                .and_then(|x| Ok((*x).try_into()?))
                .context("timestamp_criteria_max_allowed_lag")?,
            l1_batch_min_age_before_execute_seconds: self.l1_batch_min_age_before_execute_seconds,
            max_acceptable_priority_fee_in_gwei: *required(
                &self.max_acceptable_priority_fee_in_gwei,
            )
            .context("max_acceptable_priority_fee_in_gwei")?,
            pubdata_sending_mode: required(&self.pubdata_sending_mode)
                .and_then(|x| Ok(proto::PubdataSendingMode::try_from(*x)?))
                .context("pubdata_sending_mode")?
                .parse(),
            ignore_db_nonce: None,
            priority_tree_start_index: self.priority_op_start_index.map(|x| x as usize),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            aggregated_proof_sizes: this
                .aggregated_proof_sizes
                .iter()
                .map(|x| (*x).try_into().unwrap())
                .collect(),
            wait_confirmations: this.wait_confirmations,
            tx_poll_period: Some(this.tx_poll_period),
            aggregate_tx_poll_period: Some(this.aggregate_tx_poll_period),
            max_txs_in_flight: Some(this.max_txs_in_flight),
            proof_sending_mode: Some(proto::ProofSendingMode::new(&this.proof_sending_mode).into()),
            max_aggregated_tx_gas: Some(this.max_aggregated_tx_gas),
            max_eth_tx_data_size: Some(this.max_eth_tx_data_size.try_into().unwrap()),
            max_aggregated_blocks_to_commit: Some(this.max_aggregated_blocks_to_commit),
            max_aggregated_blocks_to_execute: Some(this.max_aggregated_blocks_to_execute),
            aggregated_block_commit_deadline: Some(this.aggregated_block_commit_deadline),
            aggregated_block_prove_deadline: Some(this.aggregated_block_prove_deadline),
            aggregated_block_execute_deadline: Some(this.aggregated_block_execute_deadline),
            timestamp_criteria_max_allowed_lag: Some(
                this.timestamp_criteria_max_allowed_lag.try_into().unwrap(),
            ),
            l1_batch_min_age_before_execute_seconds: this.l1_batch_min_age_before_execute_seconds,
            max_acceptable_priority_fee_in_gwei: Some(this.max_acceptable_priority_fee_in_gwei),
            pubdata_sending_mode: Some(
                proto::PubdataSendingMode::new(&this.pubdata_sending_mode).into(),
            ),
            priority_op_start_index: this.priority_tree_start_index.map(|x| x as u64),
        }
    }
}

impl ProtoRepr for proto::GasAdjuster {
    type Type = configs::eth_sender::GasAdjusterConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            default_priority_fee_per_gas: *required(&self.default_priority_fee_per_gas)
                .context("default_priority_fee_per_gas")?,
            max_base_fee_samples: required(&self.max_base_fee_samples)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_base_fee_samples")?,
            pricing_formula_parameter_a: *required(&self.pricing_formula_parameter_a)
                .context("pricing_formula_parameter_a")?,
            pricing_formula_parameter_b: *required(&self.pricing_formula_parameter_b)
                .context("pricing_formula_parameter_b")?,
            internal_l1_pricing_multiplier: *required(&self.internal_l1_pricing_multiplier)
                .context("internal_l1_pricing_multiplier")?,
            internal_enforced_l1_gas_price: self.internal_enforced_l1_gas_price,
            internal_enforced_pubdata_price: self.internal_enforced_pubdata_price,
            poll_period: *required(&self.poll_period).context("poll_period")?,
            max_l1_gas_price: self.max_l1_gas_price,
            num_samples_for_blob_base_fee_estimate: required(
                &self.num_samples_for_blob_base_fee_estimate,
            )
            .and_then(|x| Ok((*x).try_into()?))
            .context("num_samples_for_blob_base_fee_estimate")?,
            internal_pubdata_pricing_multiplier: *required(
                &self.internal_pubdata_pricing_multiplier,
            )
            .context("internal_pubdata_pricing_multiplier")?,
            max_blob_base_fee: self.max_blob_base_fee,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            default_priority_fee_per_gas: Some(this.default_priority_fee_per_gas),
            max_base_fee_samples: Some(this.max_base_fee_samples.try_into().unwrap()),
            pricing_formula_parameter_a: Some(this.pricing_formula_parameter_a),
            pricing_formula_parameter_b: Some(this.pricing_formula_parameter_b),
            internal_l1_pricing_multiplier: Some(this.internal_l1_pricing_multiplier),
            internal_enforced_l1_gas_price: this.internal_enforced_l1_gas_price,
            internal_enforced_pubdata_price: this.internal_enforced_pubdata_price,
            poll_period: Some(this.poll_period),
            max_l1_gas_price: this.max_l1_gas_price,
            num_samples_for_blob_base_fee_estimate: Some(
                this.num_samples_for_blob_base_fee_estimate
                    .try_into()
                    .unwrap(),
            ),
            internal_pubdata_pricing_multiplier: Some(this.internal_pubdata_pricing_multiplier),
            max_blob_base_fee: this.max_blob_base_fee,
        }
    }
}

impl ProtoRepr for proto::EthWatch {
    type Type = configs::EthWatchConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            confirmations_for_eth_event: self.confirmations_for_eth_event,
            eth_node_poll_interval: *required(&self.eth_node_poll_interval)
                .context("eth_node_poll_interval")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            confirmations_for_eth_event: this.confirmations_for_eth_event,
            eth_node_poll_interval: Some(this.eth_node_poll_interval),
        }
    }
}
