use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::required;

use crate::{
    proto,
    repr::{read_required_repr, ProtoRepr},
};

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

impl proto::ProofLoadingMode {
    fn new(x: &configs::eth_sender::ProofLoadingMode) -> Self {
        use configs::eth_sender::ProofLoadingMode as From;
        match x {
            From::OldProofFromDb => Self::OldProofFromDb,
            From::FriProofFromGcs => Self::FriProofFromGcs,
        }
    }

    fn parse(&self) -> configs::eth_sender::ProofLoadingMode {
        use configs::eth_sender::ProofLoadingMode as To;
        match self {
            Self::OldProofFromDb => To::OldProofFromDb,
            Self::FriProofFromGcs => To::FriProofFromGcs,
        }
    }
}

impl ProtoRepr for proto::EthSender {
    type Type = configs::eth_sender::ETHSenderConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            sender: read_required_repr(&self.sender).context("sender")?,
            gas_adjuster: read_required_repr(&self.gas_adjuster).context("gas_adjuster")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            sender: Some(ProtoRepr::build(&this.sender)),
            gas_adjuster: Some(ProtoRepr::build(&this.gas_adjuster)),
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
            proof_loading_mode: required(&self.proof_loading_mode)
                .and_then(|x| Ok(proto::ProofLoadingMode::try_from(*x)?))
                .context("proof_loading_mode")?
                .parse(),
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
            proof_loading_mode: Some(proto::ProofLoadingMode::new(&this.proof_loading_mode).into()),
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
            poll_period: *required(&self.poll_period).context("poll_period")?,
            max_l1_gas_price: self.max_l1_gas_price,
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
            poll_period: Some(this.poll_period),
            max_l1_gas_price: this.max_l1_gas_price,
        }
    }
}
