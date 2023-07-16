use crate::application::AppError;

use super::application::App;
use std::string::ToString;
use zksync_dal::{self, prover_dal::GetProverJobsParams};
use zksync_types::{
    proofs::{ProverJobInfo, ProverJobStatus},
    L1BatchNumber,
};

pub struct ProverStats {
    pub successful: usize,
    pub successful_padding: L1BatchNumber,
    pub in_progress: usize,
    pub queued: usize,
    pub queued_padding: L1BatchNumber,
    pub failed: usize,
    pub jobs: Vec<ProverJobInfo>,
}

pub fn get_stats(app: &mut App) -> Result<ProverStats, AppError> {
    let handle = app.tokio.handle();
    let stats = handle.block_on(app.db.prover_dal().get_prover_jobs_stats());
    let stats_extended = handle
        .block_on(app.db.prover_dal().get_extended_stats())
        .map_err(|x| AppError::Db(x.to_string()))?;

    Ok(ProverStats {
        successful: stats.successful,
        successful_padding: stats_extended.successful_padding,
        in_progress: stats.in_progress,
        queued: stats.queued,
        queued_padding: stats_extended.queued_padding,
        failed: stats.failed,
        jobs: stats_extended.active_area,
    })
}

pub fn print_stats(stats: &ProverStats, term_width: u32) -> Result<(), AppError> {
    struct Map {
        //width: u32,
        successful: Vec<bool>,
        in_progress: Vec<bool>,
        queued: Vec<bool>,
        failed: Vec<bool>,
        skipped: Vec<bool>,
    }

    impl Map {
        fn new(
            display_w: u32,
            active_area_start: u32,
            active_area_size: u32,
            jobs: &[ProverJobInfo],
        ) -> Map {
            let mut s: Vec<_> = (0..display_w).map(|_| false).collect();
            let mut i: Vec<_> = (0..display_w).map(|_| false).collect();
            let mut q: Vec<_> = (0..display_w).map(|_| false).collect();
            let mut f: Vec<_> = (0..display_w).map(|_| false).collect();
            let mut sk: Vec<_> = (0..display_w).map(|_| false).collect();

            let area_term_ratio = active_area_size as f32 / display_w as f32;

            for j in jobs
                .iter()
                .filter(|x| !matches!(x.status, ProverJobStatus::Ignored))
            {
                let ix = ((j.block_number.0 - active_area_start) as f32 / area_term_ratio) as usize;

                (match j.status {
                    ProverJobStatus::Successful(_) => &mut s,
                    ProverJobStatus::InProgress(_) => &mut i,
                    ProverJobStatus::Queued => &mut q,
                    ProverJobStatus::Failed(_) => &mut f,
                    ProverJobStatus::Skipped => &mut sk,
                    _ => unreachable!(),
                })[ix] = true;
            }

            Map {
                successful: s,
                failed: f,
                in_progress: i,
                queued: q,
                skipped: sk,
            }
        }
    }

    let active_area_start = stats.successful_padding.0 + 1;
    let active_area_size = stats.queued_padding.0 - active_area_start;

    let display_w = std::cmp::min(term_width, active_area_size);

    let map = Map::new(display_w, active_area_start, active_area_size, &stats.jobs);

    let map_fn = |x: &bool| match x {
        true => "+",
        false => ".",
    };

    let to_str_fn = |v: Vec<_>| v.iter().map(map_fn).collect::<String>();

    println!("Prover jobs: ");
    println!("  Queued: {}", stats.queued);
    println!("  In progress: {}", stats.in_progress);
    println!(
        "  Successful: {}, block reach: {}",
        stats.successful, stats.successful_padding
    );
    println!("  Failed: {}", stats.failed);

    if stats.failed > 0 {
        println!("      [id:block] circuit type")
    }

    for x in stats
        .jobs
        .iter()
        .filter(|x| matches!(x.status, ProverJobStatus::Failed(_)))
    {
        println!("    - [{}:{}] {}", x.id, x.block_number, x.circuit_type)
    }

    println!();
    println!(
        "Active area [{} - {}] ({})",
        stats.successful_padding.0 + 1,
        stats.queued_padding.0 - 1,
        stats.queued_padding.0 - stats.successful_padding.0 - 2,
    );
    println!("q: --|{}|--", to_str_fn(map.queued));
    println!("i: --|{}|--", to_str_fn(map.in_progress));
    println!("s: --|{}|--", to_str_fn(map.successful));
    println!("f: --|{}|--", to_str_fn(map.failed));
    println!("x: --|{}|--", to_str_fn(map.skipped));

    Ok(())
}

pub fn get_jobs(app: &mut App, opts: GetProverJobsParams) -> Result<Vec<ProverJobInfo>, AppError> {
    let handle = app.tokio.handle();
    handle
        .block_on(app.db.prover_dal().get_jobs(opts))
        .map_err(|x| AppError::Db(x.to_string()))
}

pub fn print_jobs(jobs: &[ProverJobInfo]) -> Result<(), AppError> {
    fn pji2string(job: &ProverJobInfo) -> String {
        format!(
            "Id: {}
Block: {}
Circuit type: {}
Aggregation round: {}
Status: {}",
            job.id,
            job.block_number,
            job.circuit_type,
            job.position.aggregation_round as u32,
            job.status
        )
    }

    let results = jobs.iter().map(pji2string).collect::<Vec<_>>();

    println!("{}\n\n{} results", results.join("\n\n"), results.len());

    Ok(())
}
