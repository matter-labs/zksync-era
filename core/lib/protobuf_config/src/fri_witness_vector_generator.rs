use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{repr::ProtoRepr, required};

use crate::proto;

impl ProtoRepr for proto::FriWitnessVectorGenerator {
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
