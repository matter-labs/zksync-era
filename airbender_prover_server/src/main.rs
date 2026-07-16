mod client;
mod jobs;
mod types;
mod worker;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use airbender_host::SecurityLevel;
use anyhow::{Context, Result};
use clap::Parser;
use eravm_prover_host::{
    app_bin_path, app_text_path, build_fri_prover, deserialize_from_file, FriProverConfig,
    FriVerifier, SnarkOptions, SnarkPipeline, SnarkWrapperVK,
};
use tracing::info;
use zksync_cli_utils::init_tracing;

use client::JobServerClient;
use jobs::JobWorker;
use types::ProverMode;
use worker::{ProverWorker, ProverWorkerBuilder};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Prover server: polls for jobs and submits prove results"
)]
struct Cli {
    /// Base URL of the job server (e.g. http://localhost:8080)
    #[arg(long, env = "PROVER_SERVER_URL")]
    server_url: String,

    /// Pipeline this prover runs:
    /// `fri-only` (default) — proves FRI, submits FRI;
    /// `fri-snark` — proves FRI + SNARK, submits both;
    /// `snark-only` — wraps FRI proofs into SNARKs, submits SNARK.
    #[arg(long, env = "PROVER_MODE", value_enum, default_value_t = ProverMode::FriOnly)]
    mode: ProverMode,

    /// How long to wait between polls when no job is available (milliseconds)
    #[arg(long, env = "PROVER_POLL_INTERVAL_MS", default_value = "5000")]
    poll_interval_ms: u64,

    /// Number of worker threads for the GPU FRI prover (no effect in a
    /// CUDA-free `snark-only` build).
    #[arg(long, env = "PROVER_WORKER_THREADS")]
    worker_threads: Option<usize>,

    /// Cap (in GiB) on the GPU memory the FRI prover's device allocator claims.
    /// By default it grabs all free VRAM, leaving no room for the in-process
    /// SNARK wrapper in `fri-snark` mode. Set this (e.g. 32) so both fit on one
    /// card. A value at or above free memory is a no-op.
    #[arg(long, env = "FRI_GPU_MEMORY_GB")]
    fri_gpu_memory_gb: Option<f64>,

    /// Pinned host transfer buffers the FRI prover pre-allocates per concurrent
    /// job, each a 64 MiB page-locked (committed) allocation. The pool is
    /// `per-job + per-device` buffers total; on a single-GPU box that is
    /// `this + --fri-host-buffers-per-device`. Upstream default is 256; lowered
    /// to reclaim committed RAM (the pool is mostly prefetch headroom). Raise
    /// back toward 256 if FRI proving throughput regresses.
    #[arg(long, env = "FRI_HOST_BUFFERS_PER_JOB", default_value = "224")]
    fri_host_buffers_per_job: usize,

    /// Pinned host transfer buffers the FRI prover pre-allocates per GPU device,
    /// each 64 MiB. Upstream default is 128; lowered to reclaim committed RAM.
    #[arg(long, env = "FRI_HOST_BUFFERS_PER_DEVICE", default_value = "64")]
    fri_host_buffers_per_device: usize,

    /// Identifier for this prover instance, included in proof submissions.
    /// Defaults to the HOSTNAME environment variable (i.e. the Kubernetes pod name).
    #[arg(long, env = "PROVER_ID", default_value_t = default_prover_id())]
    prover_id: String,

    /// Number of attempts to submit a prove result before giving up
    #[arg(long, env = "PROVER_SUBMIT_ATTEMPTS", default_value = "3")]
    submit_attempts: usize,

    /// TCP connect timeout for HTTP calls to the job server (milliseconds)
    #[arg(long, env = "PROVER_HTTP_CONNECT_TIMEOUT_MS", default_value = "5000")]
    http_connect_timeout_ms: u64,

    /// Per-request timeout for polling job inputs (milliseconds)
    #[arg(long, env = "PROVER_POLL_TIMEOUT_MS", default_value = "30000")]
    poll_timeout_ms: u64,

    /// Per-request timeout for submitting proof results (milliseconds).
    /// SNARK submissions can be large, so this is generally larger than the poll timeout.
    #[arg(long, env = "PROVER_SUBMIT_TIMEOUT_MS", default_value = "120000")]
    submit_timeout_ms: u64,

    /// Port to expose Prometheus metrics on (disabled if not set)
    #[arg(long, env = "PROVER_METRICS_PORT")]
    metrics_port: Option<u16>,

    /// Path to the compiled guest program directory
    #[arg(long, env = "PROVER_GUEST_DIST_DIR")]
    guest_dist_dir: Option<PathBuf>,

    /// Path to the committed FRI verification key (bincode). The server
    /// hard-fails at startup if this file is missing — it never derives the
    /// VK on the fly. Regenerate with `eravm-prover-host gen-vks` when the
    /// guest binary changes.
    #[arg(long, env = "FRI_VK", default_value_os_t = default_fri_vk_path())]
    fri_vk: PathBuf,

    /// Path to the trusted setup (CRS) for the SNARK wrapper.
    /// Required when `--mode` is `fri-snark` or `snark-only`.
    #[arg(
        long,
        env = "SNARK_TRUSTED_SETUP_FILE",
        required_if_eq_any = [("mode", "fri-snark"), ("mode", "snark-only")],
    )]
    snark_trusted_setup: Option<PathBuf>,

    /// Use a zero-knowledge SNARK wrapping path. Off by default.
    #[arg(long, env = "SNARK_USE_ZK")]
    snark_use_zk: bool,

    /// Worker threads for the SNARK wrapper (defaults to wrapper's own default).
    #[arg(long, env = "SNARK_THREADS")]
    snark_threads: Option<usize>,

    /// Path to the committed SNARK VK JSON. Defaults to the vendored
    /// `vks/snark_vk.json`; the server never derives it on the fly. Regenerate
    /// with `eravm-prover-host gen-vks` when the guest changes. Only consumed in
    /// `fri-snark` / `snark-only` modes.
    #[arg(long, env = "SNARK_VK", default_value_os_t = default_snark_vk_path())]
    snark_vk: PathBuf,

    /// Sentry DSN for error reporting. When unset, Sentry is disabled and the
    /// server logs to stdout only.
    #[arg(long, env = "SENTRY_URL")]
    sentry_url: Option<String>,

    /// Deployment environment reported to Sentry (e.g. `era-stage-proofs`).
    #[arg(long, env = "SENTRY_ENVIRONMENT")]
    sentry_environment: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize Sentry before anything else so early failures are captured.
    // The returned guard must live for the whole program; dropping it flushes
    // any buffered events.
    let _sentry_guard = init_sentry(&cli);

    init_tracing()?;

    if let Some(port) = cli.metrics_port {
        zksync_prover_metrics::start_metrics_server(port)
            .context("while starting metrics server")?;
        info!(port, "Metrics server started");
    }

    let dist_dir = cli.guest_dist_dir.clone().unwrap_or_else(default_dist_dir);
    let security = SecurityLevel::default();

    let prover_builder = build_prover(&cli, &dist_dir, security)?;

    let poll_interval = Duration::from_millis(cli.poll_interval_ms);

    // Channel capacity 1: the job worker can buffer one job ahead while the prover is busy.
    let (job_tx, job_rx) = mpsc::sync_channel(1);
    // Channel capacity 2 to accommodate `fri-snark` mode emitting two results per job.
    let (result_tx, result_rx) = mpsc::sync_channel(2);

    let shutdown = Arc::new(AtomicBool::new(false));
    ctrlc::set_handler({
        let shutdown = Arc::clone(&shutdown);
        move || {
            info!("Shutdown signal received, stopping after current job...");
            shutdown.store(true, Ordering::Relaxed);
        }
    })
    .context("while setting termination signal handler")?;

    info!(
        server_url = %cli.server_url,
        mode = ?cli.mode,
        "Starting prover server"
    );

    let prover = prover_builder
        .build(job_rx, result_tx)
        .context("while building prover worker")?;
    // SNARK wrapper recursion needs much more stack than Rust's 2 MB
    // `std::thread::spawn` default. `ulimit -s unlimited` (and the README
    // guidance) only affects the main thread, not spawned ones, so we set the
    // stack explicitly here. 128 MB is generous virtual mem; only the used
    // portion gets backed by physical pages.
    const PROVER_THREAD_STACK_SIZE: usize = 128 * 1024 * 1024;
    let prover_handle: std::thread::JoinHandle<()> = std::thread::Builder::new()
        .name("prover".to_owned())
        .stack_size(PROVER_THREAD_STACK_SIZE)
        .spawn(move || prover.run())
        .context("while spawning prover thread")?;

    let client = JobServerClient::new(
        cli.prover_id,
        cli.submit_attempts,
        cli.server_url,
        Duration::from_millis(cli.http_connect_timeout_ms),
        Duration::from_millis(cli.poll_timeout_ms),
        Duration::from_millis(cli.submit_timeout_ms),
    )
    .context("while building job server client")?;

    let job_worker_handle: std::thread::JoinHandle<()> = std::thread::spawn(move || {
        JobWorker::new(client, job_tx, result_rx, shutdown, cli.mode, poll_interval).run()
    });

    info!("Waiting for prover to finish current job...");
    prover_handle.join().expect("prover thread panicked");
    job_worker_handle
        .join()
        .expect("job worker thread panicked");
    Ok(())
}

fn build_prover(
    cli: &Cli,
    dist_dir: &std::path::Path,
    security: SecurityLevel,
) -> Result<ProverWorkerBuilder> {
    let snark_options = SnarkOptions {
        worker_threads: cli.snark_threads,
        trusted_setup: cli.snark_trusted_setup.clone(),
        use_zk: cli.snark_use_zk,
        // Server path drives the wrapper directly and never persists intermediates.
        save_intermediates: false,
        // The wrapper binds the guest program into the SNARK; use the FRI prover's guest dist dir.
        bin: app_bin_path(dist_dir),
        text: app_text_path(dist_dir),
    };

    // Backend-agnostic FRI prover config from the server's flags. The host
    // transfer-buffer pool defaults below the upstream 256/128 to reclaim
    // committed pinned RAM; raise it back if FRI throughput regresses.
    // `build_fri_prover` is a CUDA-free build's stub that errors, so `fri-only`
    // / `fri-snark` fail there with a clear message — no `#[cfg]` needed here.
    let build_fri = || {
        let fri_config = FriProverConfig {
            worker_threads: cli.worker_threads,
            max_device_memory_gb: cli.fri_gpu_memory_gb,
            host_buffers_per_job: cli.fri_host_buffers_per_job,
            host_buffers_per_device: cli.fri_host_buffers_per_device,
        };
        build_fri_prover(dist_dir, &cli.fri_vk, security, fri_config)
            .context("while building FRI prover")
    };
    let build_snark = || -> Result<SnarkPipeline> {
        let snark_vk = load_snark_vk(&cli.snark_vk)?;
        SnarkPipeline::new(&snark_options, Some(snark_vk)).context("while building SNARK pipeline")
    };

    let builder = ProverWorker::builder();
    Ok(match cli.mode {
        ProverMode::FriOnly => builder.with_fri(build_fri()?),
        ProverMode::FriSnark => builder.with_fri(build_fri()?).with_snark(build_snark()?),
        ProverMode::SnarkOnly => {
            // The worker doesn't run the FRI prover here, but the FRI proofs we
            // receive from the job server still have to be verified before we
            // burn cycles wrapping them into a SNARK.
            let verifier = FriVerifier::load(dist_dir, &cli.fri_vk, security)
                .context("while building FRI verifier for snark-only mode")?;
            builder
                .with_fri_verifier(verifier)
                .with_snark(build_snark()?)
        }
    })
}

fn load_snark_vk(path: &std::path::Path) -> Result<SnarkWrapperVK> {
    let path_string = path.to_string_lossy().into_owned();
    let vk: SnarkWrapperVK = deserialize_from_file(&path_string)
        .with_context(|| format!("while loading SNARK VK from {}", path.display()))?;
    info!(path = %path.display(), "Loaded SNARK VK from file");
    Ok(vk)
}

/// Initialize Sentry error reporting. Returns `None` (Sentry disabled) when no
/// DSN is configured. The caller must keep the returned guard alive for the
/// program's lifetime — its `Drop` flushes buffered events. Initializing Sentry
/// also installs a panic handler, so panics on any thread are reported.
fn init_sentry(cli: &Cli) -> Option<sentry::ClientInitGuard> {
    // Treat an unset *or* empty `SENTRY_URL` as "Sentry disabled". An empty
    // value is common when a deployment templates the var but leaves it blank,
    // and we don't want that to look like a misconfigured DSN.
    let dsn = cli
        .sentry_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let guard = sentry::init((
        dsn,
        sentry::ClientOptions {
            release: Some(env!("CARGO_PKG_VERSION").into()),
            environment: cli.sentry_environment.clone().map(Into::into),
            ..Default::default()
        },
    ));
    if guard.is_enabled() {
        eprintln!(
            "Sentry initialized with DSN {}, environment {:?}",
            dsn, cli.sentry_environment
        );
        Some(guard)
    } else {
        // An invalid DSN leaves the client disabled; keeping the guard would be
        // misleading. Tracing isn't up yet, so report on stderr directly.
        eprintln!("Sentry DSN provided but client is disabled; error reporting is off");
        None
    }
}

fn default_prover_id() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_owned())
}

/// Default guest program dir (`app.bin` / `app.text`), downloaded by `build.rs`
/// from the matching verifier release. Override with `--guest-dist-dir` /
/// `PROVER_GUEST_DIST_DIR`.
fn default_dist_dir() -> PathBuf {
    PathBuf::from(env!("AIRBENDER_GUEST_DIST_DIR"))
}

/// Default FRI VK path, downloaded by `build.rs`. Override with `--fri-vk` /
/// `FRI_VK`.
fn default_fri_vk_path() -> PathBuf {
    PathBuf::from(env!("AIRBENDER_FRI_VK"))
}

/// Default SNARK VK path, downloaded by `build.rs`. Override with `--snark-vk` /
/// `SNARK_VK`.
fn default_snark_vk_path() -> PathBuf {
    PathBuf::from(env!("AIRBENDER_SNARK_VK"))
}
