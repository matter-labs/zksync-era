use std::collections::HashSet;

use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};
use zksync_types::basic_fri_types::CircuitIdRoundTuple;

use crate::proto::prover as proto;

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

fn read_vec(v: &[proto::CircuitIdRoundTuple]) -> anyhow::Result<HashSet<CircuitIdRoundTuple>> {
    v.iter()
        .enumerate()
        .map(|(i, x)| x.read().context(i))
        .collect()
}

fn build_vec(v: &HashSet<CircuitIdRoundTuple>) -> Vec<proto::CircuitIdRoundTuple> {
    let mut v: Vec<_> = v.iter().cloned().collect();
    v.sort();
    v.iter().map(ProtoRepr::build).collect()
}

impl ProtoRepr for proto::ProverGroup {
    type Type = configs::fri_prover_group::FriProverGroupConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            group_0: read_vec(&self.group_0).context("group_0")?,
            group_1: read_vec(&self.group_1).context("group_1")?,
            group_2: read_vec(&self.group_2).context("group_2")?,
            group_3: read_vec(&self.group_3).context("group_3")?,
            group_4: read_vec(&self.group_4).context("group_4")?,
            group_5: read_vec(&self.group_5).context("group_5")?,
            group_6: read_vec(&self.group_6).context("group_6")?,
            group_7: read_vec(&self.group_7).context("group_7")?,
            group_8: read_vec(&self.group_8).context("group_8")?,
            group_9: read_vec(&self.group_9).context("group_9")?,
            group_10: read_vec(&self.group_10).context("group_10")?,
            group_11: read_vec(&self.group_11).context("group_11")?,
            group_12: read_vec(&self.group_12).context("group_12")?,
            group_13: read_vec(&self.group_13).context("group_13")?,
            group_14: read_vec(&self.group_14).context("group_14")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            group_0: build_vec(&this.group_0),
            group_1: build_vec(&this.group_1),
            group_2: build_vec(&this.group_2),
            group_3: build_vec(&this.group_3),
            group_4: build_vec(&this.group_4),
            group_5: build_vec(&this.group_5),
            group_6: build_vec(&this.group_6),
            group_7: build_vec(&this.group_7),
            group_8: build_vec(&this.group_8),
            group_9: build_vec(&this.group_9),
            group_10: build_vec(&this.group_10),
            group_11: build_vec(&this.group_11),
            group_12: build_vec(&this.group_12),
            group_13: build_vec(&this.group_13),
            group_14: build_vec(&this.group_14),
        }
    }
}

impl ProtoRepr for proto::ProverGateway {
    type Type = configs::FriProverGatewayConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
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
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            api_url: Some(this.api_url.clone()),
            api_poll_duration_secs: Some(this.api_poll_duration_secs.into()),
            prometheus_listener_port: Some(this.prometheus_listener_port.into()),
            prometheus_pushgateway_url: Some(this.prometheus_pushgateway_url.clone()),
            prometheus_push_interval_ms: this.prometheus_push_interval_ms,
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
            shall_save_to_public_bucket: *required(&self.shall_save_to_public_bucket)
                .context("shall_save_to_public_bucket")?,
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
            shall_save_to_public_bucket: Some(this.shall_save_to_public_bucket),
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

impl ProtoRepr for proto::WitnessVectorGenerator {
    type Type = configs::FriWitnessVectorGeneratorConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            max_prover_reservation_duration_in_secs: required(
                &self.max_prover_reservation_duration_in_secs,
            )
            .and_then(|x| Ok((*x).try_into()?))
            .context("max_prover_reservation_duration_in_secs")?,
            prover_instance_wait_timeout_in_secs: required(
                &self.prover_instance_wait_timeout_in_secs,
            )
            .and_then(|x| Ok((*x).try_into()?))
            .context("prover_instance_wait_timeout_in_secs")?,
            prover_instance_poll_time_in_milli_secs: required(
                &self.prover_instance_poll_time_in_milli_secs,
            )
            .and_then(|x| Ok((*x).try_into()?))
            .context("prover_instance_poll_time_in_milli_secs")?,
            prometheus_listener_port: required(&self.prometheus_listener_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("prometheus_listener_port")?,
            prometheus_pushgateway_url: required(&self.prometheus_pushgateway_url)
                .context("prometheus_pushgateway_url")?
                .clone(),
            prometheus_push_interval_ms: self.prometheus_push_interval_ms,
            specialized_group_id: required(&self.specialized_group_id)
                .and_then(|x| Ok((*x).try_into()?))
                .context("specialized_group_id")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            max_prover_reservation_duration_in_secs: Some(
                this.max_prover_reservation_duration_in_secs.into(),
            ),
            prover_instance_wait_timeout_in_secs: Some(
                this.prover_instance_wait_timeout_in_secs.into(),
            ),
            prover_instance_poll_time_in_milli_secs: Some(
                this.prover_instance_poll_time_in_milli_secs.into(),
            ),
            prometheus_listener_port: Some(this.prometheus_listener_port.into()),
            prometheus_pushgateway_url: Some(this.prometheus_pushgateway_url.clone()),
            prometheus_push_interval_ms: this.prometheus_push_interval_ms,
            specialized_group_id: Some(this.specialized_group_id.into()),
        }
    }
}

impl proto::SetupLoadMode {
    fn new(x: &configs::fri_prover::SetupLoadMode) -> Self {
        use configs::fri_prover::SetupLoadMode as From;
        match x {
            From::FromDisk => Self::FromDisk,
            From::FromMemory => Self::FromMemory,
        }
    }

    fn parse(&self) -> configs::fri_prover::SetupLoadMode {
        use configs::fri_prover::SetupLoadMode as To;
        match self {
            Self::FromDisk => To::FromDisk,
            Self::FromMemory => To::FromMemory,
        }
    }
}

impl proto::CloudType {
    fn new(x: &configs::fri_prover::CloudConnectionMode) -> Self {
        use configs::fri_prover::CloudConnectionMode as From;
        match x {
            From::GCP => Self::Gcp,
            From::Local => Self::Local,
        }
    }

    fn parse(&self) -> configs::fri_prover::CloudConnectionMode {
        use configs::fri_prover::CloudConnectionMode as To;
        match self {
            Self::Gcp => To::GCP,
            Self::Local => To::Local,
        }
    }
}

impl ProtoRepr for proto::Prover {
    type Type = configs::FriProverConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        let public_object_store = if let Some(object_store) = &self.public_object_store {
            Some(object_store.read()?)
        } else {
            None
        };
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
            setup_load_mode: required(&self.setup_load_mode)
                .and_then(|x| Ok(proto::SetupLoadMode::try_from(*x)?))
                .context("setup_load_mode")?
                .parse(),
            specialized_group_id: required(&self.specialized_group_id)
                .and_then(|x| Ok((*x).try_into()?))
                .context("specialized_group_id")?,
            queue_capacity: required(&self.queue_capacity)
                .and_then(|x| Ok((*x).try_into()?))
                .context("queue_capacity")?,
            witness_vector_receiver_port: required(&self.witness_vector_receiver_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("witness_vector_receiver_port")?,
            zone_read_url: required(&self.zone_read_url)
                .context("zone_read_url")?
                .clone(),
            availability_check_interval_in_secs: self.availability_check_interval_in_secs,
            shall_save_to_public_bucket: *required(&self.shall_save_to_public_bucket)
                .context("shall_save_to_public_bucket")?,
            public_object_store,
            prover_object_store,
            cloud_type: self
                .cloud_type
                .map(proto::CloudType::try_from)
                .transpose()
                .context("cloud_type")?
                .map(|x| x.parse())
                .unwrap_or_default(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            setup_data_path: Some(this.setup_data_path.clone()),
            prometheus_port: Some(this.prometheus_port.into()),
            max_attempts: Some(this.max_attempts),
            generation_timeout_in_secs: Some(this.generation_timeout_in_secs.into()),
            setup_load_mode: Some(proto::SetupLoadMode::new(&this.setup_load_mode).into()),
            specialized_group_id: Some(this.specialized_group_id.into()),
            queue_capacity: Some(this.queue_capacity.try_into().unwrap()),
            witness_vector_receiver_port: Some(this.witness_vector_receiver_port.into()),
            zone_read_url: Some(this.zone_read_url.clone()),
            availability_check_interval_in_secs: this.availability_check_interval_in_secs,
            shall_save_to_public_bucket: Some(this.shall_save_to_public_bucket),
            prover_object_store: this.prover_object_store.as_ref().map(ProtoRepr::build),
            public_object_store: this.public_object_store.as_ref().map(ProtoRepr::build),
            cloud_type: Some(proto::CloudType::new(&this.cloud_type).into()),
        }
    }
}
