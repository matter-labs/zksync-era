use anyhow::Context as _;
use zksync_config::configs;
use zksync_protobuf::{read_optional, repr::ProtoRepr, required, ProtoFmt};

use crate::{proto::prover_autoscaler as proto, read_optional_repr};

impl ProtoRepr for proto::GeneralConfig {
    type Type = configs::prover_autoscaler::GeneralConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            graceful_shutdown_timeout: read_optional(&self.graceful_shutdown_timeout)
                .context("graceful_shutdown_timeout")?
                .unwrap_or(Self::Type::default_graceful_shutdown_timeout()),
            agent_config: read_optional_repr(&self.agent_config),
            scaler_config: read_optional_repr(&self.scaler_config),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            graceful_shutdown_timeout: Some(ProtoFmt::build(&this.graceful_shutdown_timeout)),
            agent_config: this.agent_config.as_ref().map(ProtoRepr::build),
            scaler_config: this.scaler_config.as_ref().map(ProtoRepr::build),
        }
    }
}

impl ProtoRepr for proto::ProverAutoscalerAgentConfig {
    type Type = configs::prover_autoscaler::ProverAutoscalerAgentConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            prometheus_port: required(&self.prometheus_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("prometheus_port")?,
            http_port: required(&self.http_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("http_port")?,
            namespaces: self.namespaces.iter().cloned().collect(),
            cluster_name: Some("".to_string()),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            prometheus_port: Some(this.prometheus_port.into()),
            http_port: Some(this.http_port.into()),
            namespaces: this.namespaces.clone(),
        }
    }
}

impl ProtoRepr for proto::ProverAutoscalerScalerConfig {
    type Type = configs::prover_autoscaler::ProverAutoscalerScalerConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            prometheus_port: required(&self.prometheus_port)
                .and_then(|x| Ok((*x).try_into()?))
                .context("prometheus_port")?,
            scaler_run_interval: read_optional(&self.scaler_run_interval)
                .context("scaler_run_interval")?
                .unwrap_or(Self::Type::default_scaler_run_interval()),
            prover_job_monitor_url: required(&self.prover_job_monitor_url)
                .context("prover_job_monitor_url")?
                .clone(),
            agents: self.agents.iter().cloned().collect(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            prometheus_port: Some(this.prometheus_port.into()),
            scaler_run_interval: Some(ProtoFmt::build(&this.scaler_run_interval)),
            prover_job_monitor_url: Some(this.prover_job_monitor_url.clone()),
            agents: this.agents.clone(),
        }
    }
}
