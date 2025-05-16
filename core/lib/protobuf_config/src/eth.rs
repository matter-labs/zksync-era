use anyhow::Context as _;
use zksync_config::configs::{self};
use zksync_protobuf::{required, ProtoRepr};
use zksync_types::pubdata_da::PubdataSendingMode;

use crate::{proto::eth as proto, read_optional_repr};

impl proto::GasLimitMode {
    fn new(x: &configs::eth_sender::GasLimitMode) -> Self {
        use configs::eth_sender::GasLimitMode as From;
        match x {
            From::Maximum => proto::GasLimitMode::Maximum,
            From::Calculated => proto::GasLimitMode::Calculated,
        }
    }

    fn parse(&self) -> configs::eth_sender::GasLimitMode {
        use configs::eth_sender::GasLimitMode as TO;
        match self {
            Self::Maximum => TO::Maximum,
            Self::Calculated => TO::Calculated,
        }
    }
}

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
    fn new(x: &PubdataSendingMode) -> Self {
        match x {
            PubdataSendingMode::Calldata => Self::Calldata,
            PubdataSendingMode::Blobs => Self::Blobs,
            PubdataSendingMode::Custom => Self::Custom,
            PubdataSendingMode::RelayedL2Calldata => Self::RelayedL2Calldata,
        }
    }

    fn parse(&self) -> PubdataSendingMode {
        match self {
            Self::Calldata => PubdataSendingMode::Calldata,
            Self::Blobs => PubdataSendingMode::Blobs,
            Self::Custom => PubdataSendingMode::Custom,
            Self::RelayedL2Calldata => PubdataSendingMode::RelayedL2Calldata,
        }
    }
}

impl ProtoRepr for proto::Eth {
    type Type = configs::eth_sender::EthConfig;

    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type::new(
            read_optional_repr(&self.sender),
            read_optional_repr(&self.gas_adjuster),
            read_optional_repr(&self.watcher),
        ))
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            sender: this
                .get_eth_sender_config_for_sender_layer_data_layer()
                .map(ProtoRepr::build),
            gas_adjuster: this.gas_adjuster.as_ref().map(ProtoRepr::build),
            watcher: this.watcher.as_ref().map(ProtoRepr::build),
        }
    }
}

impl ProtoRepr for proto::Sender {
    type Type = configs::eth_sender::SenderConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
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
            tx_aggregation_only_prove_and_execute: self.tx_aggregation_paused.unwrap_or(false),
            tx_aggregation_paused: self.tx_aggregation_only_prove_and_execute.unwrap_or(false),
            time_in_mempool_in_l1_blocks_cap: self
                .time_in_mempool_in_l1_blocks_cap
                .unwrap_or(Self::Type::default_time_in_mempool_in_l1_blocks_cap()),
            is_verifier_pre_fflonk: self.is_verifier_pre_fflonk.unwrap_or(true),
            gas_limit_mode: self
                .gas_limit_mode
                .map(proto::GasLimitMode::try_from)
                .transpose()
                .context("gas_limit_mode")?
                .map(|a| a.parse())
                .unwrap_or(Self::Type::default_gas_limit_mode()),
            max_acceptable_base_fee_in_wei: self
                .max_acceptable_base_fee_in_wei
                .unwrap_or(Self::Type::default_max_acceptable_base_fee_in_wei()),
            precommit_params: self.precommit_params.as_ref().map(|x| {
                configs::eth_sender::PrecommitParams {
                    l2_blocks_to_aggregate: *required(&x.l2_blocks_to_aggregate).unwrap(),
                    deadline_sec: *required(&x.deadline_sec).unwrap(),
                }
            }),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
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
            tx_aggregation_only_prove_and_execute: Some(this.tx_aggregation_only_prove_and_execute),
            tx_aggregation_paused: Some(this.tx_aggregation_paused),
            time_in_mempool_in_l1_blocks_cap: Some(this.time_in_mempool_in_l1_blocks_cap),
            is_verifier_pre_fflonk: Some(this.is_verifier_pre_fflonk),
            gas_limit_mode: Some(proto::GasLimitMode::new(&this.gas_limit_mode).into()),
            max_acceptable_base_fee_in_wei: Some(this.max_acceptable_base_fee_in_wei),
            precommit_params: this
                .precommit_params
                .as_ref()
                .map(|x| proto::PrecommitParams {
                    l2_blocks_to_aggregate: Some(x.l2_blocks_to_aggregate),
                    deadline_sec: Some(x.deadline_sec),
                }),
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
                .unwrap_or(&Self::Type::default_pricing_formula_parameter_a()),
            pricing_formula_parameter_b: *required(&self.pricing_formula_parameter_b)
                .unwrap_or(&Self::Type::default_pricing_formula_parameter_b()),
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
