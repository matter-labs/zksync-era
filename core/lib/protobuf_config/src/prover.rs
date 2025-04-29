use anyhow::Context as _;
use zksync_basic_types::basic_fri_types::CircuitIdRoundTuple;
use zksync_config::configs::{self, fri_prover_gateway::ApiMode};
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto::prover::{self as proto};

impl ProtoRepr for proto::ProofCompressor {
    type Type = configs::FriProofCompressorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            compression_mode: required(&self.compression_mode)
                .and_then(|x| Ok((*x).try_into()?))
                .context("compression_mode")?,
            prometheus_listener_port: required(&self.prometheus_listener_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("prometheus_listener_port")?,
            prometheus_pushgateway_url: required(&self.prometheus_pushgateway_url)
                .context("prometheus_pushgateway_url")?
                .clone(),
            prometheus_push_interval_ms: self.prometheus_push_interval_ms,
            generation_timeout_in_secs: required(&self.generation_timeout_in_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("generation_timeout_in_secs")?,
            max_attempts: *required(&self.max_attempts).context("max_attempts")?,
            universal_setup_path: required(&self.universal_setup_path)
                .context("universal_setup_path")?
                .clone(),
            universal_setup_download_url: required(&self.universal_setup_download_url)
                .context("universal_setup_download_url")?
                .clone(),
            verify_wrapper_proof: *required(&self.verify_wrapper_proof)
                .context("verify_wrapper_proof")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            compression_mode: Some(this.compression_mode.into()),
            prometheus_listener_port: Some(this.prometheus_listener_port.into()),
            prometheus_pushgateway_url: Some(this.prometheus_pushgateway_url.clone()),
            prometheus_push_interval_ms: this.prometheus_push_interval_ms,
            generation_timeout_in_secs: Some(this.generation_timeout_in_secs.into()),
            max_attempts: Some(this.max_attempts),
            universal_setup_path: Some(this.universal_setup_path.clone()),
            universal_setup_download_url: Some(this.universal_setup_download_url.clone()),
            verify_wrapper_proof: Some(this.verify_wrapper_proof),
        }
    }
}

impl ProtoRepr for proto::CircuitIdRoundTuple {
    type Type = CircuitIdRoundTuple;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            circuit_id: required(&self.circuit_id)
                .and_then(|x| Ok((*x).try_into()?))
                .context("circuit_id")?,
            aggregation_round: required(&self.aggregation_round)
                .and_then(|x| Ok((*x).try_into()?))
                .context("aggregation_round")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            circuit_id: Some(this.circuit_id.into()),
            aggregation_round: Some(this.aggregation_round.into()),
        }
    }
}

impl ProtoRepr for proto::ProverGateway {
    type Type = configs::FriProverGatewayConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let api_mode = if let Some(api_mode) = &self.api_mode {
            match api_mode {
                0 => ApiMode::Legacy,
                1 => ApiMode::ProverCluster,
                _ => panic!("Unknown ProofDataHandler API mode: {api_mode}"),
            }
        } else {
            ApiMode::default()
        };

        Ok(Self::Type {
            api_url: required(&self.api_url).context("api_url")?.clone(),
            api_poll_duration_secs: required(&self.api_poll_duration_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("api_poll_duration_secs")?,
            prometheus_listener_port: required(&self.prometheus_listener_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("prometheus_listener_port")?,
            prometheus_pushgateway_url: required(&self.prometheus_pushgateway_url)
                .context("prometheus_pushgateway_url")?
                .clone(),
            prometheus_push_interval_ms: self.prometheus_push_interval_ms,
            api_mode,
            port: self.port.map(|x| x as u16),
        })
    }

    fn build(this: &Self::Type) -> Self {
        let api_mode = match this.api_mode {
            ApiMode::Legacy => 0,
            ApiMode::ProverCluster => 1,
        };

        Self {
            api_url: Some(this.api_url.clone()),
            api_poll_duration_secs: Some(this.api_poll_duration_secs.into()),
            prometheus_listener_port: Some(this.prometheus_listener_port.into()),
            prometheus_pushgateway_url: Some(this.prometheus_pushgateway_url.clone()),
            prometheus_push_interval_ms: this.prometheus_push_interval_ms,
            api_mode: Some(api_mode),
            port: this.port.map(|x| x as u32),
        }
    }
}

impl ProtoRepr for proto::WitnessGenerator {
    type Type = configs::FriWitnessGeneratorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            generation_timeout_in_secs: required(&self.generation_timeout_in_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("generation_timeout_in_secs")?,
            max_attempts: *required(&self.max_attempts).context("max_attempts")?,
            last_l1_batch_to_process: self.last_l1_batch_to_process,
            basic_generation_timeout_in_secs: self
                .basic_generation_timeout_in_secs
                .map(|x| x.try_into())
                .transpose()
                .context("basic_generation_timeout_in_secs")?,
            leaf_generation_timeout_in_secs: self
                .leaf_generation_timeout_in_secs
                .map(|x| x.try_into())
                .transpose()
                .context("leaf_generation_timeout_in_secs")?,
            node_generation_timeout_in_secs: self
                .node_generation_timeout_in_secs
                .map(|x| x.try_into())
                .transpose()
                .context("node_generation_timeout_in_secs")?,
            recursion_tip_generation_timeout_in_secs: self
                .recursion_tip_timeout_in_secs
                .map(|x| x.try_into())
                .transpose()
                .context("recursion_tip_generation_timeout_in_secs")?,
            scheduler_generation_timeout_in_secs: self
                .scheduler_generation_timeout_in_secs
                .map(|x| x.try_into())
                .transpose()
                .context("scheduler_generation_timeout_in_secs")?,
            prometheus_listener_port: self
                .prometheus_listener_port
                .map(|x| x.try_into())
                .transpose()
                .context("prometheus_listener_port")?,
            max_circuits_in_flight: required(&self.max_circuits_in_flight)
                .and_then(|x| Ok((*x).try_into()?))
                .context("max_circuits_in_flight")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            generation_timeout_in_secs: Some(this.generation_timeout_in_secs.into()),
            max_attempts: Some(this.max_attempts),
            last_l1_batch_to_process: this.last_l1_batch_to_process,
            basic_generation_timeout_in_secs: this
                .basic_generation_timeout_in_secs
                .map(|x| x.into()),
            leaf_generation_timeout_in_secs: this.leaf_generation_timeout_in_secs.map(|x| x.into()),
            node_generation_timeout_in_secs: this.node_generation_timeout_in_secs.map(|x| x.into()),
            recursion_tip_timeout_in_secs: this
                .recursion_tip_generation_timeout_in_secs
                .map(|x| x.into()),
            scheduler_generation_timeout_in_secs: this
                .scheduler_generation_timeout_in_secs
                .map(|x| x.into()),
            prometheus_listener_port: this.prometheus_listener_port.map(|x| x.into()),
            max_circuits_in_flight: Some(this.max_circuits_in_flight as u64),
        }
    }
}

impl ProtoRepr for proto::Prover {
    type Type = configs::FriProverConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let prover_object_store = if let Some(object_store) = &self.prover_object_store {
            Some(object_store.read()?)
        } else {
            None
        };

        Ok(Self::Type {
            setup_data_path: required(&self.setup_data_path)
                .context("setup_data_path")?
                .clone(),
            prometheus_port: required(&self.prometheus_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("prometheus_port")?,
            max_attempts: *required(&self.max_attempts).context("max_attempts")?,
            generation_timeout_in_secs: required(&self.generation_timeout_in_secs)
                .and_then(|x| Ok((*x).try_into()?))
                .context("generation_timeout_in_secs")?,
            prover_object_store,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            setup_data_path: Some(this.setup_data_path.clone()),
            prometheus_port: Some(this.prometheus_port.into()),
            max_attempts: Some(this.max_attempts),
            generation_timeout_in_secs: Some(this.generation_timeout_in_secs.into()),
            prover_object_store: this.prover_object_store.as_ref().map(ProtoRepr::build),
        }
    }
}
