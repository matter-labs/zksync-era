use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto;

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

impl ProtoRepr for proto::FriProver {
    type Type = configs::FriProverConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
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
            base_layer_circuit_ids_to_be_verified: required(
                &self.base_layer_circuit_ids_to_be_verified,
            )
            .context("base_layer_circuit_ids_to_be_verified")?
            .clone(),
            recursive_layer_circuit_ids_to_be_verified: required(
                &self.recursive_layer_circuit_ids_to_be_verified,
            )
            .context("recursive_layer_circuit_ids_to_be_verified")?
            .clone(),
            setup_load_mode: required(&self.setup_load_mode)
                .and_then(|x| Ok(proto::SetupLoadMode::try_from(*x)?))
                .context("setup_load_mode")?
                .parse(),
            specialized_group_id: required(&self.specialized_group_id)
                .and_then(|x| Ok((*x).try_into()?))
                .context("specialized_group_id")?,
            witness_vector_generator_thread_count: self
                .witness_vector_generator_thread_count
                .map(|x| x.try_into())
                .transpose()
                .context("witness_vector_generator_thread_count")?,
            queue_capacity: required(&self.queue_capacity)
                .and_then(|x| Ok((*x).try_into()?))
                .context("queue_capacity")?,
            witness_vector_receiver_port: required(&self.witness_vector_receiver_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("witness_vector_receiver_port")?,
            zone_read_url: required(&self.zone_read_url)
                .context("zone_read_url")?
                .clone(),
            shall_save_to_public_bucket: *required(&self.shall_save_to_public_bucket)
                .context("shall_save_to_public_bucket")?,
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            setup_data_path: Some(this.setup_data_path.clone()),
            prometheus_port: Some(this.prometheus_port.into()),
            max_attempts: Some(this.max_attempts),
            generation_timeout_in_secs: Some(this.generation_timeout_in_secs.into()),
            base_layer_circuit_ids_to_be_verified: Some(
                this.base_layer_circuit_ids_to_be_verified.clone(),
            ),
            recursive_layer_circuit_ids_to_be_verified: Some(
                this.recursive_layer_circuit_ids_to_be_verified.clone(),
            ),
            setup_load_mode: Some(proto::SetupLoadMode::new(&this.setup_load_mode).into()),
            specialized_group_id: Some(this.specialized_group_id.into()),
            witness_vector_generator_thread_count: this
                .witness_vector_generator_thread_count
                .map(|x| x.try_into().unwrap()),
            queue_capacity: Some(this.queue_capacity.try_into().unwrap()),
            witness_vector_receiver_port: Some(this.witness_vector_receiver_port.into()),
            zone_read_url: Some(this.zone_read_url.clone()),
            shall_save_to_public_bucket: Some(this.shall_save_to_public_bucket),
        }
    }
}
