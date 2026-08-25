//! Airbender autoscaler: a single-binary Kubernetes-style controller.
//!
//! Reconcile loop: observe (queue reports + cluster state) → plan (pure
//! function, `planner.rs`) → act (patch deployment scale). Dry-run computes
//! and logs every decision without acting. Errors in one group or one tick
//! never crash the loop — the next tick retries from fresh observations.

mod config;
mod kube;
mod planner;
mod queue;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{error, info, warn};

use crate::config::{Config, Mode};
use crate::kube::ClusterSet;
use crate::planner::{plan, Decision, DeploymentState, PlanInput};
use crate::queue::QueueCollector;

#[derive(Debug, Parser)]
#[command(about, version)]
struct Cli {
    /// Path to the YAML config file.
    #[arg(long, env = "AUTOSCALER_CONFIG", default_value = "config.yaml")]
    config: PathBuf,
    /// Force dry-run regardless of the config file.
    #[arg(long)]
    dry_run: bool,
    /// Run a single reconcile tick and exit (useful with --dry-run).
    #[arg(long)]
    once: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let config = Config::load(&cli.config)?;
    let dry_run = cli.dry_run || config.dry_run;
    info!(
        config = %cli.config.display(),
        dry_run,
        groups = config.groups.len(),
        "starting airbender autoscaler"
    );

    let clusters = ClusterSet::connect(&config.clusters, &config.namespace)
        .await
        .context("connecting to clusters")?;
    let mut collector = QueueCollector::new()?;
    // Hysteresis state per (group, mode); in-memory only — a restart
    // conservatively re-confirms demand before scaling.
    let mut states: HashMap<(String, Mode), DeploymentState> = HashMap::new();

    loop {
        if let Err(err) = reconcile(&config, &clusters, &mut collector, &mut states, dry_run).await
        {
            error!(?err, "reconcile tick failed");
        }
        if cli.once {
            return Ok(());
        }
        tokio::select! {
            _ = tokio::time::sleep(config.reconcile_interval) => {}
            _ = tokio::signal::ctrl_c() => {
                info!("shutdown signal received");
                return Ok(());
            }
        }
    }
}

async fn reconcile(
    config: &Config,
    clusters: &ClusterSet,
    collector: &mut QueueCollector,
    states: &mut HashMap<(String, Mode), DeploymentState>,
    dry_run: bool,
) -> Result<()> {
    let now = Instant::now();
    for (group_name, group) in &config.groups {
        let queues = collector.collect(group).await;
        if let Some(q) = &queues {
            info!(
                group = %group_name,
                fri_ready = q.fri.ready,
                fri_in_progress = q.fri.in_progress,
                fri_reclaimable = q.fri.reclaimable,
                snark_ready = q.snark.ready,
                snark_in_progress = q.snark.in_progress,
                snark_reclaimable = q.snark.reclaimable,
                "queue report"
            );
        }

        // Deployments in `Mode` order: snark-only first, fri-only last, so a
        // tight budget protects SNARK capacity and cuts fri-only first.
        let mut budget_remaining = group.max_gpus;
        for (&mode, deployment_config) in &group.deployments {
            let deployment_name = deployment_config.resolved_name(mode, group_name);
            let observations = match clusters.observe(&deployment_name).await {
                Ok(obs) => obs,
                Err(err) => {
                    warn!(
                        group = %group_name,
                        deployment = %deployment_name,
                        ?err,
                        "failed to observe deployment; skipping this tick"
                    );
                    continue;
                }
            };
            if observations.is_empty() {
                warn!(
                    group = %group_name,
                    deployment = %deployment_name,
                    "deployment not found in any cluster; create it (0 replicas is fine)"
                );
                continue;
            }

            let state = states.entry((group_name.clone(), mode)).or_default();
            let decision = plan(
                &PlanInput {
                    mode,
                    deployment: deployment_config,
                    group,
                    queues: queues.as_ref(),
                    clusters: &observations,
                    budget_remaining,
                    now,
                },
                state,
            );

            let spec_total: u32 = observations.iter().map(|c| c.spec_replicas).sum();
            match &decision {
                Decision::Hold { reason } => {
                    info!(
                        group = %group_name,
                        deployment = %deployment_name,
                        replicas = spec_total,
                        decision = "hold",
                        %reason,
                        "planned"
                    );
                    budget_remaining = budget_remaining.saturating_sub(spec_total);
                }
                Decision::Scale {
                    cluster,
                    to,
                    reason,
                } => {
                    let from = observations
                        .iter()
                        .find(|c| &c.cluster == cluster)
                        .map(|c| c.spec_replicas)
                        .unwrap_or(0);
                    info!(
                        group = %group_name,
                        deployment = %deployment_name,
                        %cluster,
                        from,
                        to,
                        decision = if dry_run { "scale (dry-run)" } else { "scale" },
                        %reason,
                        "planned"
                    );
                    if !dry_run {
                        if let Err(err) = clusters.scale(cluster, &deployment_name, *to).await {
                            error!(
                                group = %group_name,
                                deployment = %deployment_name,
                                %cluster,
                                ?err,
                                "scale request failed; will replan next tick"
                            );
                        }
                    }
                    let new_total = spec_total - from + to;
                    budget_remaining = budget_remaining.saturating_sub(new_total);
                }
            }
        }
    }
    Ok(())
}
