use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use anyhow::Context;
use zksync_prover_task::Task;

use super::{
    queuer,
    scaler::{CapMode, Scaler, ScalerConfig, ScalerTrait},
    watcher,
};
use crate::{
    agent::ScaleRequest,
    cluster_types::{ClusterName, NamespaceName},
    config::{ProverAutoscalerScalerConfig, QueueReportFields, ScalerTargetType},
    key::{GpuKey, NoKey},
    metrics::AUTOSCALER_METRICS,
};

pub struct Manager {
    /// Namespace to Protocol Version configuration.
    namespaces: HashMap<NamespaceName, String>,
    watcher: watcher::Watcher,
    queuer: queuer::Queuer,
    first_invoke_skipped: AtomicBool,
    jobs: Vec<QueueReportFields>,
    scalers: Vec<Box<dyn ScalerTrait + Sync + Send>>,
}

impl Manager {
    pub fn new(
        watcher: watcher::Watcher,
        queuer: queuer::Queuer,
        config: ProverAutoscalerScalerConfig,
    ) -> Self {
        config
            .protocol_versions
            .iter()
            .for_each(|(namespace, version)| {
                AUTOSCALER_METRICS.prover_protocol_version[&(namespace.clone(), version.clone())]
                    .set(1);
            });

        let mut scalers: Vec<Box<dyn ScalerTrait + Sync + Send>> = Vec::default();
        let mut jobs = Vec::default();

        let scaler_config = Arc::new(ScalerConfig {
            cluster_priorities: config.cluster_priorities,
            apply_min_to_namespace: config.apply_min_to_namespace,
            long_pending_duration: chrono::Duration::seconds(
                config.long_pending_duration.as_secs() as i64,
            ),
            scale_errors_duration: chrono::Duration::seconds(
                config.scale_errors_duration.as_secs() as i64,
            ),
            aggressive_mode_threshold: config.aggressive_mode_threshold,
            aggressive_mode_cooldown: chrono::Duration::seconds(
                config.aggressive_mode_cooldown.as_secs() as i64,
            ),
        });

        for c in &config.scaler_targets {
            jobs.push(c.queue_report_field);
            match c.scaler_target_type {
                ScalerTargetType::Gpu => scalers.push(Box::new(Scaler::<GpuKey>::new(
                    c.queue_report_field,
                    c.deployment.clone(),
                    c.min_replicas,
                    c.max_replicas
                        .iter()
                        .map(|(k, v)| (k.clone(), v.into_map_gpukey()))
                        .collect(),
                    c.speed.into_map_gpukey(),
                    c.max_running_weight,
                    c.max_desired_burst_weight,
                    c.hysteresis,
                    scaler_config.clone(),
                    c.priority.clone(),
                ))),
                ScalerTargetType::Simple => scalers.push(Box::new(Scaler::<NoKey>::new(
                    c.queue_report_field,
                    c.deployment.clone(),
                    c.min_replicas,
                    c.max_replicas
                        .iter()
                        .map(|(k, v)| (k.clone(), v.into_map_nokey()))
                        .collect(),
                    c.speed.into_map_nokey(),
                    c.max_running_weight,
                    c.max_desired_burst_weight,
                    c.hysteresis,
                    scaler_config.clone(),
                    c.priority.clone(),
                ))),
            };
        }
        Self {
            namespaces: config.protocol_versions.clone(),
            watcher,
            queuer,
            first_invoke_skipped: AtomicBool::new(false),
            jobs,
            scalers,
        }
    }
}

#[async_trait::async_trait]
impl Task for Manager {
    async fn invoke(&self) -> anyhow::Result<()> {
        if !self.first_invoke_skipped.load(Ordering::Relaxed) {
            self.first_invoke_skipped.store(true, Ordering::Relaxed);
            return Ok(());
        }

        let queue = self
            .queuer
            .get_queue(&self.jobs)
            .await
            .context("Failed to get the queue")?;

        let mut scale_requests: HashMap<ClusterName, ScaleRequest> = HashMap::new();
        {
            let guard = self.watcher.data.lock().await; // Keeping the lock during all calls of run() for
                                                        // consistency.
            if let Err(err) = self.watcher.check_is_ready(&guard) {
                tracing::error!("Skipping Manager run: {}", err);
                return Ok(());
            }

            for scaler in &self.scalers {
                let mut namespace_queues: Vec<_> = self
                    .namespaces
                    .iter()
                    .map(|(ns, ppv)| {
                        let q = queue
                            .get(&(ppv.to_string(), scaler.queue_report_field()))
                            .cloned()
                            .unwrap_or(0);
                        AUTOSCALER_METRICS.queue[&(ns.clone(), scaler.deployment())].set(q);
                        (ns.clone(), ppv.clone(), q)
                    })
                    .collect();

                // Busiest namespace first so it gets priority for the shared cap.
                namespace_queues.sort_by(|(ns_a, _, q_a), (ns_b, _, q_b)| {
                    q_b.cmp(q_a).then_with(|| ns_a.cmp(ns_b))
                });

                // Compute per-namespace running weights and aggregate.
                let ns_running_weights: Vec<usize> = namespace_queues
                    .iter()
                    .map(|(ns, _, _)| scaler.current_running_weight(ns, &guard.clusters))
                    .collect();
                let total_running_weight: usize = ns_running_weights.iter().sum();
                let total_queue: usize = namespace_queues.iter().map(|(_, _, q)| *q).sum();
                let all_namespaces: Vec<_> = namespace_queues
                    .iter()
                    .map(|(ns, _, _)| ns.clone())
                    .collect();

                // Evaluate aggressive mode once with the worst-case view.
                scaler.evaluate_aggressive_mode(
                    &all_namespaces,
                    &guard.clusters,
                    total_running_weight,
                    total_queue,
                );

                // Determine cap mode based on total running weight.
                let cap_mode = scaler.max_running().and_then(|max_running| {
                    let max_with_burst = scaler.max_desired_weight().unwrap_or(max_running);
                    if total_running_weight >= max_with_burst {
                        Some(CapMode::ScaleDown {
                            target_weight: max_with_burst,
                        })
                    } else if total_running_weight >= max_running {
                        Some(CapMode::FreezeAtRunning)
                    } else {
                        None
                    }
                });

                for (i, (ns, _ppv, q)) in namespace_queues.iter().enumerate() {
                    tracing::debug!(
                        "Running eval for namespace {ns}, scaler {} found queue {q}, total_running_weight {total_running_weight}, cap_mode {:?}",
                        scaler.deployment(),
                        cap_mode
                    );
                    scaler.run(
                        ns,
                        *q,
                        &guard.clusters,
                        &mut scale_requests,
                        cap_mode,
                        ns_running_weights[i],
                    );
                }
            }
        } // Unlock self.watcher.data.

        if let Err(err) = self.watcher.send_scale(scale_requests).await {
            tracing::error!("Failed scale request: {}", err);
        }

        Ok(())
    }
}
