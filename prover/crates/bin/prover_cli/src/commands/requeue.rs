use anyhow::Context;
use clap::Args as ClapArgs;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_types::{
    basic_fri_types::AggregationRound, prover_dal::StuckJobs, L1BatchId, L1BatchNumber, L2ChainId,
};

use crate::cli::ProverCLIConfig;

#[derive(ClapArgs)]
pub struct Args {
    #[clap(short, long)]
    batch: L1BatchNumber,
    /// Maximum number of attempts to re-queue a job.
    /// Default value is 10.
    /// NOTE: this argument is temporary and will be deprecated once the `config` command is implemented.
    #[clap(long, default_value_t = 10)]
    max_attempts: u32,
}

pub async fn run(args: Args, config: ProverCLIConfig) -> anyhow::Result<()> {
    let pool = ConnectionPool::<Prover>::singleton(config.db_url)
        .build()
        .await
        .context("failed to build a prover_connection_pool")?;

    let mut conn = pool
        .connection()
        .await
        .context("failed to acquire a connection")?;

    let stuck_witness_input_jobs = conn
        .fri_basic_witness_generator_dal()
        .requeue_stuck_witness_inputs_jobs_for_batch(
            L1BatchId::new(L2ChainId::zero(), args.batch),
            args.max_attempts,
        )
        .await;
    display_requeued_stuck_jobs(stuck_witness_input_jobs, AggregationRound::BasicCircuits);

    let stuck_leaf_aggregations_stuck_jobs = conn
        .fri_witness_generator_dal()
        .requeue_stuck_leaf_aggregation_jobs_for_batch(
            L1BatchId::new(L2ChainId::zero(), args.batch),
            args.max_attempts,
        )
        .await;
    display_requeued_stuck_jobs(
        stuck_leaf_aggregations_stuck_jobs,
        AggregationRound::LeafAggregation,
    );

    let stuck_node_aggregations_jobs = conn
        .fri_witness_generator_dal()
        .requeue_stuck_node_aggregation_jobs_for_batch(
            L1BatchId::new(L2ChainId::zero(), args.batch),
            args.max_attempts,
        )
        .await;
    display_requeued_stuck_jobs(
        stuck_node_aggregations_jobs,
        AggregationRound::NodeAggregation,
    );

    let stuck_recursion_tip_job = conn
        .fri_recursion_tip_witness_generator_dal()
        .requeue_stuck_recursion_tip_jobs_for_batch(
            L1BatchId::new(L2ChainId::zero(), args.batch),
            args.max_attempts,
        )
        .await;
    display_requeued_stuck_jobs(stuck_recursion_tip_job, AggregationRound::RecursionTip);

    let stuck_scheduler_jobs = conn
        .fri_scheduler_witness_generator_dal()
        .requeue_stuck_scheduler_jobs_for_batch(
            L1BatchId::new(L2ChainId::zero(), args.batch),
            args.max_attempts,
        )
        .await;
    display_requeued_stuck_jobs(stuck_scheduler_jobs, AggregationRound::Scheduler);

    let stuck_proof_compressor_jobs = conn
        .fri_proof_compressor_dal()
        .requeue_stuck_jobs_for_batch(
            L1BatchId::new(L2ChainId::zero(), args.batch),
            args.max_attempts,
        )
        .await;
    for stuck_job in stuck_proof_compressor_jobs {
        println!("Re-queuing proof compressor job {stuck_job:?} 🔁",);
    }

    let stuck_prover_jobs = conn
        .fri_prover_jobs_dal()
        .requeue_stuck_jobs_for_batch(
            L1BatchId::new(L2ChainId::zero(), args.batch),
            args.max_attempts,
        )
        .await;

    for stuck_job in stuck_prover_jobs {
        println!("Re-queuing prover job {stuck_job:?} 🔁",);
    }

    Ok(())
}

fn display_requeued_stuck_jobs(stuck_jobs: Vec<StuckJobs>, aggregation_round: AggregationRound) {
    for stuck_job in stuck_jobs {
        println!("Re-queuing {aggregation_round} stuck job {stuck_job:?} 🔁",);
    }
}
