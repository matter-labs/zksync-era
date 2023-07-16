use std::cmp::{max, min};
use std::ops::Add;
use std::{collections::HashMap, convert::TryFrom, iter};

use std::convert::AsRef;

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use zksync_dal::prover_dal::GetProverJobsParams;
use zksync_dal::witness_generator_dal::GetWitnessJobsParams;
use zksync_types::proofs::{ProverJobStatus, WitnessJobStatus};

use zksync_types::{
    proofs::{AggregationRound, ProverJobInfo, WitnessJobInfo},
    L1BatchNumber,
};

use crate::application::{App, AppError};

pub struct RoundWitnessStats {
    job: WitnessJobInfo,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub struct RoundProverStats {
    jobs: Vec<ProverJobInfo>,
    updated_at: DateTime<Utc>,
}

pub struct AggregationRoundInfo {
    prover: Option<RoundProverStats>,
    witness: Option<RoundWitnessStats>,
    round_number: AggregationRound,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub struct BlockInfo {
    id: L1BatchNumber,
    aggregations: Vec<Option<AggregationRoundInfo>>,
}

pub fn get_block_info(id: L1BatchNumber, app: &mut App) -> Result<BlockInfo, AppError> {
    /// Assumes that all provided jobs are from the same aggregation round.
    fn jobs_to_round_stats(
        witness: Option<WitnessJobInfo>,
        proofs: Vec<ProverJobInfo>,
    ) -> (Option<RoundWitnessStats>, Option<RoundProverStats>) {
        let witness = witness.map(move |x| RoundWitnessStats {
            created_at: x.created_at,
            updated_at: x.updated_at,
            job: x,
        });

        let prover = proofs
            .iter()
            .map(|x| x.updated_at)
            .reduce(max)
            .map(move |updated_at| RoundProverStats {
                jobs: proofs,
                updated_at,
            });

        (witness, prover)
    }

    fn compose_round(
        round: AggregationRound,
        witness: Option<RoundWitnessStats>,
        prover: Option<RoundProverStats>,
    ) -> Option<AggregationRoundInfo> {
        witness.map(move |witness| AggregationRoundInfo {
            round_number: round,
            created_at: witness.created_at,
            updated_at: prover
                .as_ref()
                .map(|x| x.updated_at)
                .unwrap_or_else(|| witness.updated_at),
            witness: Some(witness),
            prover,
        })
    }

    let handle = app.tokio.handle();

    let witness_jobs = handle
        .block_on(
            app.db
                .witness_generator_dal()
                .get_jobs(GetWitnessJobsParams {
                    blocks: Some(id..id),
                }),
        )
        .map_err(|x| AppError::Db(x.to_string()))?;

    let proof_jobs = handle
        .block_on(
            app.db
                .prover_dal()
                .get_jobs(GetProverJobsParams::blocks(id..id)),
        )
        .map_err(|x| AppError::Db(x.to_string()))?;

    let mut proof_groups =
        proof_jobs
            .into_iter()
            .fold(HashMap::<_, Vec<_>>::new(), |mut aggr, cur| {
                aggr.entry(cur.position.aggregation_round)
                    .or_default()
                    .push(cur);
                aggr
            });

    let mut witns_groups = witness_jobs
        .into_iter()
        .fold(HashMap::new(), |mut aggr, cur| {
            if aggr.insert(cur.position.aggregation_round, cur).is_some() {
                panic!("Single witness job expected per generation round")
            }
            aggr
        });

    let aggregations = (0..3)
        .map(|x| AggregationRound::try_from(x).unwrap())
        .map(|ar| {
            (
                ar,
                witns_groups.remove(&ar),
                proof_groups.remove(&ar).unwrap_or_default(),
            )
        })
        .map(|(ar, wtns, prfs)| (ar, jobs_to_round_stats(wtns, prfs)))
        .map(|(ar, (w, p))| compose_round(ar, w, p))
        .collect::<Vec<_>>();

    Ok(BlockInfo { id, aggregations })
}

pub fn print_block_info(block: &BlockInfo) -> Result<(), AppError> {
    fn indent(s: &str, i: usize) -> String {
        let width = " ".repeat(i);

        String::new()
            .add(width.as_str())
            .add(&s.replace('\n', &String::from("\n").add(&width)))
    }

    fn timef(t: DateTime<Utc>) -> String {
        format!(
            "{:02}:{:02}:{:02} {}-{:02}-{:02}",
            t.hour(),
            t.minute(),
            t.second(),
            t.year(),
            t.month(),
            t.day()
        )
    }

    fn durf(d: Duration) -> String {
        format!("{}:{:02}m", d.num_minutes(), d.num_seconds() % 60)
    }

    fn print_existing_block(
        round: &AggregationRoundInfo,
        round_prev: &Option<AggregationRoundInfo>,
    ) {
        let (duration_reference, reference_name) = match round_prev {
            Some(round_prev) => (
                round_prev.updated_at,
                format!("U{}", round_prev.round_number as u32),
            ),
            None => (round.created_at, "C".to_string()),
        };

        let witness = match &round.witness {
            Some(witness) => {
                let status = match &witness.job.status {
                    WitnessJobStatus::Successful(x) => {
                        format!(
                            "Started: {} {}+{}
Elapsed: {}",
                            timef(x.started_at),
                            reference_name,
                            durf(x.started_at - duration_reference),
                            durf(x.time_taken)
                        )
                    }

                    _ => String::new(),
                };

                format!(
                    "   Witness job: {}\n{}",
                    witness.job.status,
                    indent(status.as_str(), 6)
                )
            }
            None => "No witness job.".to_string(),
        };

        let prover = match &round.prover {
            Some(prover) if !prover.jobs.is_empty() => {
                let statuses = prover
                    .jobs
                    .iter()
                    .fold(HashMap::new(), |mut aggr, cur| {
                        aggr.entry(cur.status.as_ref())
                            .and_modify(|x| *x += 1)
                            .or_insert(1);
                        aggr
                    })
                    .iter()
                    .map(|(status, count)| format!("{}: {}", status, count))
                    .collect::<Vec<_>>()
                    .join(", ");

                fn map<T>(f: fn(T, T) -> T, x: Option<T>, y: T) -> Option<T>
                where
                    T: Copy,
                {
                    x.map(|x| f(x, y)).or(Some(y))
                }

                let (started_min, started_max, elapsed_min, elapsed_max) = prover.jobs.iter().fold(
                    (None, None, None, None),
                    |(started_min, started_max, elapsed_min, elapsed_max), cur| match &cur.status {
                        ProverJobStatus::InProgress(s) => (
                            map(min, started_min, s.started_at),
                            map(max, started_max, s.started_at),
                            elapsed_min,
                            elapsed_max,
                        ),
                        ProverJobStatus::Successful(s) => (
                            map(min, started_min, s.started_at),
                            map(max, started_max, s.started_at),
                            map(min, elapsed_min, cur.updated_at - s.started_at),
                            map(max, elapsed_max, cur.updated_at - s.started_at),
                        ),
                        ProverJobStatus::Failed(s) => (
                            map(min, started_min, s.started_at),
                            map(max, started_max, s.started_at),
                            elapsed_min,
                            elapsed_max,
                        ),
                        _ => (started_min, started_max, elapsed_min, elapsed_max),
                    },
                );

                fn format_time(
                    t: Option<DateTime<Utc>>,
                    reference: DateTime<Utc>,
                    reference_name: &str,
                ) -> String {
                    match t {
                        Some(t) => {
                            format!("{} {}+{}", timef(t), reference_name, durf(t - reference))
                        }
                        None => "N/A".to_owned(),
                    }
                }

                let stats = format!(
                    "Started: {} (min)
         {} (max)
Elapsed: {} (min),
         {} (max)",
                    format_time(started_min, duration_reference, &reference_name),
                    format_time(started_max, duration_reference, &reference_name),
                    elapsed_min.map_or("N/A".to_owned(), durf),
                    elapsed_max.map_or("N/A".to_owned(), durf)
                );

                format!("   Prover jobs: {}\n{}", statuses, indent(&stats, 6))
            }
            _ => "No prover jobs.".to_string(),
        };
        println!("Round {}", round.round_number as u32);
        println!("[C]reated {}", timef(round.created_at));
        println!(
            "[U]pdated {} C+{}",
            timef(round.updated_at),
            durf(round.updated_at - round.created_at)
        );

        println!("{}\n{}", witness, prover);
    }

    fn print_missing_block(round_ix: usize) {
        println!("Round {} missing jobs", round_ix);
    }

    println!("Block {}", block.id);

    let prevs = iter::once(&None).chain(block.aggregations.iter()); // No need to map into Some, cause previous round must to exist.

    block
        .aggregations
        .iter()
        .zip(prevs)
        .enumerate()
        .for_each(|(i, (cur, prev))| {
            // Option in `cur` refers to conceptual existence of block data.
            // Option in `prev` signifies whether there is a previous block for `cur` block, and
            // must only be None for first element.
            assert!(i == 0 || prev.is_some());

            match cur {
                Some(x) => print_existing_block(x, prev),
                None => print_missing_block(i),
            }
        });

    Ok(())
}
