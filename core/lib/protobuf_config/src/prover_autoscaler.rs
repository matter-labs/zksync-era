use std::collections::HashMap;

use anyhow::Context;
use time::Duration;
use zksync_config::configs::{self, prover_autoscaler::Gpu};
use zksync_protobuf::{read_optional, repr::ProtoRepr, required, ProtoFmt};

use crate::{proto::prover_autoscaler as proto, read_optional_repr};

impl ProtoRepr for proto::ProverAutoscalerConfig {
    type Type = configs::prover_autoscaler::ProverAutoscalerConfig;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            graceful_shutdown_timeout: read_optional(&self.graceful_shutdown_timeout)
                .context("graceful_shutdown_timeout")?
                .unwrap_or(Self::Type::default_graceful_shutdown_timeout()),
            agent_config: read_optional_repr(&self.agent_config),
            scaler_config: read_optional_repr(&self.scaler_config),
            observability: read_optional_repr(&self.observability),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            graceful_shutdown_timeout: Some(ProtoFmt::build(&this.graceful_shutdown_timeout)),
            agent_config: this.agent_config.as_ref().map(ProtoRepr::build),
            scaler_config: this.scaler_config.as_ref().map(ProtoRepr::build),
            observability: this.observability.as_ref().map(ProtoRepr::build),
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
            namespaces: self.namespaces.to_vec(),
            cluster_name: Some("".to_string()),
            dry_run: self.dry_run.unwrap_or(Self::Type::default_dry_run()),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            prometheus_port: Some(this.prometheus_port.into()),
            http_port: Some(this.http_port.into()),
            namespaces: this.namespaces.clone(),
            cluster_name: this.cluster_name.clone(),
            dry_run: Some(this.dry_run),
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
            agents: self.agents.to_vec(),
            protocol_versions: self
                .protocol_versions
                .iter()
                .enumerate()
                .map(|(i, e)| e.read().context(i))
                .collect::<Result<_, _>>()
                .context("protocol_versions")?,
            cluster_priorities: self
                .cluster_priorities
                .iter()
                .enumerate()
                .map(|(i, e)| e.read().context(i))
                .collect::<Result<_, _>>()
                .context("cluster_priorities")?,
            prover_speed: self
                .prover_speed
                .iter()
                .enumerate()
                .map(|(i, e)| e.read().context(i))
                .collect::<Result<_, _>>()
                .context("prover_speed")?,
            long_pending_duration: match self.long_pending_duration_s {
                Some(s) => Duration::seconds(s.into()),
                None => Self::Type::default_long_pending_duration(),
            },
            max_provers: self.max_provers.iter().fold(HashMap::new(), |mut acc, e| {
                let (cluster_and_gpu, max) = e.read().expect("max_provers");
                if let Some((cluster, gpu)) = cluster_and_gpu.split_once('/') {
                    acc.entry(cluster.to_string())
                        .or_default()
                        .insert(gpu.parse().expect("max_provers/gpu"), max);
                }
                acc
            }),
            min_provers: self
                .min_provers
                .iter()
                .enumerate()
                .map(|(i, e)| e.read().context(i))
                .collect::<Result<_, _>>()
                .context("min_provers")?,
            scaler_targets: self
                .scaler_targets
                .iter()
                .enumerate()
                .map(|(i, x)| x.read().context(i).unwrap())
                .collect::<Vec<_>>(),
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            prometheus_port: Some(this.prometheus_port.into()),
            scaler_run_interval: Some(ProtoFmt::build(&this.scaler_run_interval)),
            prover_job_monitor_url: Some(this.prover_job_monitor_url.clone()),
            agents: this.agents.clone(),
            protocol_versions: this
                .protocol_versions
                .iter()
                .map(|(k, v)| proto::ProtocolVersion::build(&(k.clone(), v.clone())))
                .collect(),
            cluster_priorities: this
                .cluster_priorities
                .iter()
                .map(|(k, v)| proto::ClusterPriority::build(&(k.clone(), *v)))
                .collect(),
            prover_speed: this
                .prover_speed
                .iter()
                .map(|(k, v)| proto::ProverSpeed::build(&(*k, *v)))
                .collect(),
            long_pending_duration_s: Some(this.long_pending_duration.whole_seconds() as u32),
            max_provers: this
                .max_provers
                .iter()
                .flat_map(|(cluster, inner_map)| {
                    inner_map.iter().map(move |(gpu, max)| {
                        proto::MaxProver::build(&(format!("{}/{}", cluster, gpu), *max))
                    })
                })
                .collect(),
            min_provers: this
                .min_provers
                .iter()
                .map(|(k, v)| proto::MinProver::build(&(k.clone(), *v)))
                .collect(),
            scaler_targets: this.scaler_targets.iter().map(ProtoRepr::build).collect(),
        }
    }
}

impl ProtoRepr for proto::ProtocolVersion {
    type Type = (String, String);
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok((
            required(&self.namespace).context("namespace")?.clone(),
            required(&self.protocol_version)
                .context("protocol_version")?
                .clone(),
        ))
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            namespace: Some(this.0.clone()),
            protocol_version: Some(this.1.clone()),
        }
    }
}

impl ProtoRepr for proto::ClusterPriority {
    type Type = (String, u32);
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok((
            required(&self.cluster).context("cluster")?.clone(),
            *required(&self.priority).context("priority")?,
        ))
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            cluster: Some(this.0.clone()),
            priority: Some(this.1),
        }
    }
}

impl ProtoRepr for proto::ProverSpeed {
    type Type = (Gpu, u32);
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok((
            required(&self.gpu).context("gpu")?.parse()?,
            *required(&self.speed).context("speed")?,
        ))
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            gpu: Some(this.0.to_string()),
            speed: Some(this.1),
        }
    }
}

impl ProtoRepr for proto::MaxProver {
    type Type = (String, u32);
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok((
            required(&self.cluster_and_gpu)
                .context("cluster_and_gpu")?
                .parse()?,
            *required(&self.max).context("max")?,
        ))
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            cluster_and_gpu: Some(this.0.to_string()),
            max: Some(this.1),
        }
    }
}

impl ProtoRepr for proto::MinProver {
    type Type = (String, u32);
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok((
            required(&self.namespace).context("namespace")?.clone(),
            *required(&self.min).context("min")?,
        ))
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            namespace: Some(this.0.to_string()),
            min: Some(this.1),
        }
    }
}

impl ProtoRepr for proto::MaxReplica {
    type Type = (String, usize);
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok((
            required(&self.cluster).context("cluster")?.parse()?,
            *required(&self.max).context("max")? as usize,
        ))
    }
    fn build(this: &Self::Type) -> Self {
        Self {
            cluster: Some(this.0.to_string()),
            max: Some(this.1 as u64),
        }
    }
}

impl ProtoRepr for proto::ScalerTarget {
    type Type = configs::prover_autoscaler::ScalerTarget;
    fn read(&self) -> anyhow::Result<Self::Type> {
        Ok(Self::Type {
            queue_report_field: required(&self.queue_report_field)
                .and_then(|x| Ok((*x).parse()?))
                .context("queue_report_field")?,
            pod_name_prefix: required(&self.pod_name_prefix)
                .context("pod_name_prefix")?
                .clone(),
            max_replicas: self
                .max_replicas
                .iter()
                .enumerate()
                .map(|(i, e)| e.read().context(i))
                .collect::<Result<_, _>>()
                .context("max_replicas")?,
            speed: match self.speed {
                Some(x) => x as usize,
                None => Self::Type::default_speed(),
            },
        })
    }

    fn build(this: &Self::Type) -> Self {
        Self {
            queue_report_field: Some(this.queue_report_field.to_string()),
            pod_name_prefix: Some(this.pod_name_prefix.clone()),
            max_replicas: this
                .max_replicas
                .iter()
                .map(|(k, v)| proto::MaxReplica::build(&(k.clone(), *v)))
                .collect(),
            speed: Some(this.speed as u64),
        }
    }
}
