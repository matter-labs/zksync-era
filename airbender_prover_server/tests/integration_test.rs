//! Integration test: starts the prover server binary, serves one real batch via a local HTTP
//! server, waits for the proof(s) to be submitted, then verifies them.
//!
//! Gated by `#[ignore]` because it needs a GPU, the built guest binary, and the LFS batch
//! corpus.
//!
//! `prover_server_proves_fri_then_snark` runs `fri-only` to produce the FRI proof against
//! `/airbender/submit_proofs`, kills that prover, then starts a fresh `snark-only` prover that
//! picks the captured FRI proof up via `/airbender/snark_inputs` and submits the SNARK proof to
//! `/airbender/submit_snark_proofs`. Two sequential processes so each stage owns the whole GPU.
//! The trusted setup is fetched into the system temp dir on first run (matches the build's
//! `gpu_snark` feature — GPU `setup_compact.key` when enabled, CPU `setup_2^24.key` otherwise);
//! override the path via `IT_SNARK_TRUSTED_SETUP` to reuse a local copy.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;

use axum::{
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use tokio::sync::oneshot;

use airbender_host::{
    Proof, ProverLevel, RealVerifierBuilder, SecurityLevel, VerificationRequest, Verifier,
};
use eravm_prover_host::{
    default_trusted_setup_download_url, default_trusted_setup_path,
    download_trusted_setup_if_not_present, load_vk_from_disk, SnarkWrapperProof,
};
use zksync_airbender_verifier::types::AirbenderVerifierInput;
use zksync_airbender_verifier::Verify;

const DEFAULT_FRI_PROOF_TIMEOUT: Duration = Duration::from_secs(20 * 60);
// SNARK wrapping is the long pole on top of FRI; give it room.
const DEFAULT_SNARK_PROOF_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);
/// CUDA driver cleanup window after SIGKILL'ing the fri-only prover. Without
/// it, the snark-only prover's first `cudaMalloc` can race with the tail of
/// the dead context's reclaim and fail with `cudaErrorMemoryAllocation`.
const GPU_RECLAIM_DELAY: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Test HTTP server state and handlers
// ---------------------------------------------------------------------------

/// `(batch_number, bincode-encoded `Proof` bytes)` captured from a FRI submission.
type CapturedFriProof = Arc<Mutex<Option<(u32, Vec<u8>)>>>;

#[derive(Clone)]
struct TestServerState {
    /// Stored as the bare payload so the test mock matches the upstream
    /// zksync-era wire format (a flat struct, no version enum wrapper).
    verifier_input: Arc<AirbenderVerifierInput>,
    /// One-shot latch for `/airbender/proof_inputs`: serve the job once, then 204.
    fri_input_served: Arc<AtomicBool>,
    /// Latest FRI submission captured by `/airbender/submit_proofs`. Read by
    /// `/airbender/snark_inputs` so the snark-only prover can pick it up.
    fri_proof_capture: CapturedFriProof,
    fri_proof_sender: Arc<Mutex<Option<oneshot::Sender<Vec<u8>>>>>,
    /// One-shot latch for `/airbender/snark_inputs`: serve the captured FRI
    /// proof once, then 204. Only flips after `fri_proof_capture` is `Some`.
    snark_input_served: Arc<AtomicBool>,
    snark_proof_sender: Arc<Mutex<Option<oneshot::Sender<Vec<u8>>>>>,
}

struct BatchTestInput {
    number: u32,
    filename: String,
    verifier_input: AirbenderVerifierInput,
    expected_public_input: [u32; 8],
}

#[derive(Clone)]
struct MultiBatchTestServerState {
    batches: Arc<Vec<BatchTestInput>>,
    /// Next batch eligible for `/airbender/proof_inputs`.
    next_fri_index: Arc<Mutex<usize>>,
    /// Number of SNARK submissions received. The next FRI input is served only
    /// after the prior batch's SNARK lands, keeping the live prover sequence
    /// strictly `FRI -> SNARK -> FRI -> SNARK`.
    completed_snark_count: Arc<AtomicUsize>,
    fri_proof_senders: Arc<Mutex<HashMap<u32, oneshot::Sender<Vec<u8>>>>>,
    snark_proof_senders: Arc<Mutex<HashMap<u32, oneshot::Sender<Vec<u8>>>>>,
}

#[derive(Clone)]
struct FriFixtureGenerationState {
    batches: Arc<Vec<BatchTestInput>>,
    /// Next batch eligible for `/airbender/proof_inputs`. A single long-lived
    /// `fri-only` prover picks up batches in order from this index.
    next_fri_index: Arc<Mutex<usize>>,
    /// Number of FRI submissions received. The next FRI input is served only
    /// after the prior batch's FRI proof lands, keeping the single prover on a
    /// strict `FRI -> FRI -> ...` sequence and never handing out a batch ahead
    /// of the one still in flight.
    completed_fri_count: Arc<AtomicUsize>,
    fri_proof_senders: Arc<Mutex<HashMap<u32, oneshot::Sender<Vec<u8>>>>>,
}

#[derive(Clone)]
struct SnarkFixtureReplayState {
    fixtures: Arc<Vec<FriProofFixture>>,
    next_snark_index: Arc<Mutex<usize>>,
    completed_snark_count: Arc<AtomicUsize>,
    snark_proof_senders: Arc<Mutex<HashMap<u32, oneshot::Sender<Vec<u8>>>>>,
}

#[derive(Clone)]
struct FriProofFixture {
    batch_number: u32,
    batch_file: String,
    proof_bytes: Vec<u8>,
    expected_public_input: [u32; 8],
}

#[derive(serde::Deserialize, serde::Serialize)]
struct FriProofFixtureManifest {
    batches: Vec<FriProofFixtureManifestEntry>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
struct FriProofFixtureManifestEntry {
    batch_number: u32,
    batch_file: String,
    proof_file: String,
    proof_bytes: usize,
    expected_public_input: [u32; 8],
}

/// `POST /airbender/proof_inputs` — serves the job once, then returns 204.
async fn handle_proof_inputs(State(state): State<TestServerState>) -> impl IntoResponse {
    if state.fri_input_served.swap(true, Ordering::SeqCst) {
        return StatusCode::NO_CONTENT.into_response();
    }
    println!("[test-server] Serving job to prover");
    Json((*state.verifier_input).clone()).into_response()
}

/// `POST /airbender/snark_inputs` — once the FRI submission has been captured,
/// serves it (bincode-encoded `Proof`) to a snark-only prover exactly once;
/// before capture or after replay, returns 204.
async fn handle_snark_inputs(State(state): State<TestServerState>) -> impl IntoResponse {
    let Some((batch_number, proof_bytes)) =
        state.fri_proof_capture.lock().expect("poisoned").clone()
    else {
        return StatusCode::NO_CONTENT.into_response();
    };
    if state.snark_input_served.swap(true, Ordering::SeqCst) {
        return StatusCode::NO_CONTENT.into_response();
    }
    println!(
        "[test-server] Serving SNARK input for batch {batch_number} ({} bytes)",
        proof_bytes.len()
    );
    Json(serde_json::json!({
        "l1_batch_number": batch_number,
        "fri_proof": hex::encode(&proof_bytes),
    }))
    .into_response()
}

/// `POST /airbender/submit_proofs` — stores the FRI proof bytes and signals the test.
#[derive(serde::Deserialize)]
struct SubmitFriProofBody {
    l1_batch_number: u32,
    #[allow(dead_code)]
    prover_id: String,
    /// Hex-encoded proof bytes, matching `SubmitFriProofRequest` in the server crate.
    proof: String,
}

async fn handle_submit_proofs(
    State(state): State<TestServerState>,
    Json(body): Json<SubmitFriProofBody>,
) -> StatusCode {
    let proof_bytes = match hex::decode(&body.proof) {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    println!(
        "[test-server] Received FRI proof for batch {} ({} bytes)",
        body.l1_batch_number,
        proof_bytes.len()
    );
    // Capture for replay via `/airbender/snark_inputs`. Stored before signaling
    // the receiver so a snark-only prover that polls right after the test
    // unblocks always sees a populated capture.
    *state.fri_proof_capture.lock().expect("poisoned") =
        Some((body.l1_batch_number, proof_bytes.clone()));
    if let Some(tx) = state.fri_proof_sender.lock().expect("poisoned").take() {
        let _ = tx.send(proof_bytes);
    }
    StatusCode::OK
}

async fn handle_fri_fixture_proof_inputs(
    State(state): State<FriFixtureGenerationState>,
) -> impl IntoResponse {
    let mut next_fri_index = state.next_fri_index.lock().expect("poisoned");
    let index = *next_fri_index;
    if index >= state.batches.len() {
        return StatusCode::NO_CONTENT.into_response();
    }

    // Don't hand out batch N until batch N-1's FRI proof has been submitted, so
    // the single long-lived prover never has two FRI jobs outstanding.
    let completed_fri = state.completed_fri_count.load(Ordering::SeqCst);
    if index > completed_fri {
        return StatusCode::NO_CONTENT.into_response();
    }

    let batch = &state.batches[index];
    *next_fri_index += 1;
    println!(
        "[test-server] Serving FRI fixture job {} ({})",
        batch.number, batch.filename
    );
    Json(batch.verifier_input.clone()).into_response()
}

async fn handle_fri_fixture_submit_proofs(
    State(state): State<FriFixtureGenerationState>,
    Json(body): Json<SubmitFriProofBody>,
) -> StatusCode {
    let proof_bytes = match hex::decode(&body.proof) {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    println!(
        "[test-server] Received FRI fixture proof for batch {} ({} bytes)",
        body.l1_batch_number,
        proof_bytes.len()
    );
    let Some(tx) = state
        .fri_proof_senders
        .lock()
        .expect("poisoned")
        .remove(&body.l1_batch_number)
    else {
        return StatusCode::BAD_REQUEST;
    };
    let _ = tx.send(proof_bytes);
    // Unlock the next batch for `/airbender/proof_inputs`.
    state.completed_fri_count.fetch_add(1, Ordering::SeqCst);
    StatusCode::OK
}

/// Multi-batch `POST /airbender/proof_inputs` handler for one live
/// `fri-snark` prover. It serves batch N only after batch N-1 submitted SNARK.
async fn handle_multi_batch_proof_inputs(
    State(state): State<MultiBatchTestServerState>,
) -> impl IntoResponse {
    let mut next_fri_index = state.next_fri_index.lock().expect("poisoned");
    let index = *next_fri_index;
    if index >= state.batches.len() {
        return StatusCode::NO_CONTENT.into_response();
    }

    let completed_snarks = state.completed_snark_count.load(Ordering::SeqCst);
    if index > completed_snarks {
        return StatusCode::NO_CONTENT.into_response();
    }

    let batch = &state.batches[index];
    *next_fri_index += 1;
    println!(
        "[test-server] Serving FRI job {} ({}) to fri-snark prover",
        batch.number, batch.filename
    );
    Json(batch.verifier_input.clone()).into_response()
}

async fn handle_multi_batch_submit_proofs(
    State(state): State<MultiBatchTestServerState>,
    Json(body): Json<SubmitFriProofBody>,
) -> StatusCode {
    let proof_bytes = match hex::decode(&body.proof) {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    println!(
        "[test-server] Received FRI proof for batch {} ({} bytes)",
        body.l1_batch_number,
        proof_bytes.len()
    );
    let Some(tx) = state
        .fri_proof_senders
        .lock()
        .expect("poisoned")
        .remove(&body.l1_batch_number)
    else {
        return StatusCode::BAD_REQUEST;
    };
    let _ = tx.send(proof_bytes);
    StatusCode::OK
}

async fn handle_multi_batch_submit_snark_proofs(
    State(state): State<MultiBatchTestServerState>,
    Json(body): Json<SubmitSnarkProofBody>,
) -> StatusCode {
    // Re-encode to bytes for the proof channel.
    let proof_bytes =
        serde_json::to_vec(&body.snark_proof).expect("failed to re-encode SNARK proof");
    println!(
        "[test-server] Received SNARK proof for batch {} ({} bytes)",
        body.l1_batch_number,
        proof_bytes.len()
    );
    let Some(tx) = state
        .snark_proof_senders
        .lock()
        .expect("poisoned")
        .remove(&body.l1_batch_number)
    else {
        return StatusCode::BAD_REQUEST;
    };
    let _ = tx.send(proof_bytes);
    state.completed_snark_count.fetch_add(1, Ordering::SeqCst);
    StatusCode::OK
}

async fn handle_fixture_snark_inputs(
    State(state): State<SnarkFixtureReplayState>,
) -> impl IntoResponse {
    let mut next_snark_index = state.next_snark_index.lock().expect("poisoned");
    let index = *next_snark_index;
    if index >= state.fixtures.len() {
        return StatusCode::NO_CONTENT.into_response();
    }

    let completed_snarks = state.completed_snark_count.load(Ordering::SeqCst);
    if index > completed_snarks {
        return StatusCode::NO_CONTENT.into_response();
    }

    let fixture = &state.fixtures[index];
    *next_snark_index += 1;
    println!(
        "[test-server] Serving SNARK fixture input for batch {} ({}, {} bytes)",
        fixture.batch_number,
        fixture.batch_file,
        fixture.proof_bytes.len()
    );
    Json(serde_json::json!({
        "l1_batch_number": fixture.batch_number,
        "fri_proof": hex::encode(&fixture.proof_bytes),
    }))
    .into_response()
}

async fn handle_fixture_submit_snark_proofs(
    State(state): State<SnarkFixtureReplayState>,
    Json(body): Json<SubmitSnarkProofBody>,
) -> StatusCode {
    // Re-encode to bytes for the proof channel.
    let proof_bytes =
        serde_json::to_vec(&body.snark_proof).expect("failed to re-encode SNARK proof");
    println!(
        "[test-server] Received SNARK fixture proof for batch {} ({} bytes)",
        body.l1_batch_number,
        proof_bytes.len()
    );
    let Some(tx) = state
        .snark_proof_senders
        .lock()
        .expect("poisoned")
        .remove(&body.l1_batch_number)
    else {
        return StatusCode::BAD_REQUEST;
    };
    let _ = tx.send(proof_bytes);
    state.completed_snark_count.fetch_add(1, Ordering::SeqCst);
    StatusCode::OK
}

/// `POST /airbender/submit_snark_proofs` — stores the SNARK proof bytes and signals the test.
#[derive(serde::Deserialize)]
struct SubmitSnarkProofBody {
    l1_batch_number: u32,
    #[allow(dead_code)]
    prover_id: String,
    /// The wrapper PLONK proof, matching `SubmitSnarkProofRequest` in the server
    /// crate (and zksync-era's `SubmitAirbenderSnarkProofRequest`).
    snark_proof: SnarkWrapperProof,
}

async fn handle_submit_snark_proofs(
    State(state): State<TestServerState>,
    Json(body): Json<SubmitSnarkProofBody>,
) -> StatusCode {
    // Re-encode to bytes for the proof channel.
    let proof_bytes =
        serde_json::to_vec(&body.snark_proof).expect("failed to re-encode SNARK proof");
    println!(
        "[test-server] Received SNARK proof for batch {} ({} bytes)",
        body.l1_batch_number,
        proof_bytes.len()
    );
    if let Some(tx) = state.snark_proof_sender.lock().expect("poisoned").take() {
        let _ = tx.send(proof_bytes);
    }
    StatusCode::OK
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolution order for paths the test consumes:
/// 1. `IT_<NAME>` env var (set by CI when running a prebuilt test binary on a
///    different machine than the one that compiled it — `CARGO_MANIFEST_DIR`
///    points to the build host).
/// 2. `CARGO_MANIFEST_DIR`-relative default (the local-dev path).
fn guest_dist_dir() -> PathBuf {
    std::env::var_os("IT_GUEST_DIST_DIR")
        .map(PathBuf::from)
        // `build.rs` downloads the guest program and bakes its directory in here.
        .unwrap_or_else(|| PathBuf::from(env!("AIRBENDER_GUEST_DIST_DIR")))
}

fn batch_file_path(filename: &str) -> PathBuf {
    let dir = std::env::var_os("IT_BATCHES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata/era_mainnet_batches/binary")
        });
    dir.join(filename)
}

fn fri_fixtures_dir() -> PathBuf {
    std::env::var_os("IT_FRI_FIXTURES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("eravm-airbender-fri-fixtures"))
}

fn fri_fixture_manifest_path(dir: &std::path::Path) -> PathBuf {
    dir.join("manifest.json")
}

fn prover_server_bin() -> PathBuf {
    std::env::var_os("IT_PROVER_SERVER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_eravm-prover-server")))
}

/// Path to the committed FRI verification key. Both the spawned prover and
/// the in-test proof verifier load this file directly — the server is
/// configured to never derive a VK on the fly, so it must exist.
fn fri_vk_path() -> PathBuf {
    std::env::var_os("IT_FRI_VK")
        .map(PathBuf::from)
        // `build.rs` downloads the FRI VK and bakes its path in here.
        .unwrap_or_else(|| PathBuf::from(env!("AIRBENDER_FRI_VK")))
}

/// Path to the committed SNARK wrapper verification key. Required for the
/// `snark-only` phase of the test; not consumed in the FRI-only phase.
fn snark_vk_path() -> PathBuf {
    std::env::var_os("IT_SNARK_VK")
        .map(PathBuf::from)
        // `build.rs` downloads the SNARK VK and bakes its path in here.
        .unwrap_or_else(|| PathBuf::from(env!("AIRBENDER_SNARK_VK")))
}

/// CRS used by the SNARK wrapper. The build's `gpu_snark` feature picks the
/// right URL (GPU `setup_compact.key` vs CPU `setup_2^24.key`).
///
/// If `IT_SNARK_TRUSTED_SETUP` is set, that path is used verbatim. Otherwise
/// the file is fetched into the system temp directory on first run — keeps
/// the test self-contained without depending on `target/` being writable
/// (some CI setups build and test as different users). The cache filename
/// includes the feature suffix so CPU and GPU runs don't clobber each other.
fn snark_trusted_setup_path() -> PathBuf {
    let path = std::env::var_os("IT_SNARK_TRUSTED_SETUP")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut name = std::ffi::OsString::from("eravm-airbender-");
            name.push(default_trusted_setup_path().as_os_str());
            std::env::temp_dir().join(name)
        });

    download_trusted_setup_if_not_present(&path, default_trusted_setup_download_url())
        .expect("failed to provision SNARK trusted setup for integration test");

    path
}

/// RAII guard that kills the child process on drop.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        println!("[test] Killing prover server process (pid {})", self.0.id());
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn batch_number_from_filename(filename: &str) -> u32 {
    let raw = filename
        .strip_suffix(".bin.gz")
        .or_else(|| filename.strip_suffix(".bin"))
        .unwrap_or_else(|| panic!("batch filename must end in .bin or .bin.gz: {filename}"));
    raw.parse().unwrap_or_else(|_| {
        panic!("batch filename must start with a numeric batch number: {filename}")
    })
}

fn batch_files_from_env(default: &[&str]) -> Vec<String> {
    let raw = std::env::var("IT_BATCH_FILES").unwrap_or_else(|_| default.join(","));
    let files: Vec<_> = raw
        .split(',')
        .map(str::trim)
        .filter(|file| !file.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(
        !files.is_empty(),
        "IT_BATCH_FILES did not contain any batch files"
    );
    files
}

fn truthy_env_var(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        matches!(
            value.as_str(),
            "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
        )
    })
}

fn duration_from_env_secs(name: &str, default: Duration) -> Duration {
    let Some(value) = std::env::var_os(name) else {
        return default;
    };
    let value = value
        .to_str()
        .unwrap_or_else(|| panic!("{name} must be valid UTF-8"));
    let seconds: u64 = value
        .parse()
        .unwrap_or_else(|_| panic!("{name} must be a positive integer number of seconds"));
    assert!(seconds > 0, "{name} must be positive");
    Duration::from_secs(seconds)
}

fn fri_proof_timeout() -> Duration {
    duration_from_env_secs("IT_FRI_PROOF_TIMEOUT_SECS", DEFAULT_FRI_PROOF_TIMEOUT)
}

fn snark_proof_timeout() -> Duration {
    duration_from_env_secs("IT_SNARK_PROOF_TIMEOUT_SECS", DEFAULT_SNARK_PROOF_TIMEOUT)
}

fn fri_fixture_proof_file(batch_number: u32) -> String {
    format!("{batch_number}.fri-proof.bin")
}

fn write_fri_fixture(
    dir: &std::path::Path,
    batch: &BatchTestInput,
    proof_bytes: Vec<u8>,
) -> FriProofFixtureManifestEntry {
    std::fs::create_dir_all(dir)
        .unwrap_or_else(|err| panic!("failed to create {}: {err}", dir.display()));
    let proof_file = fri_fixture_proof_file(batch.number);
    let proof_path = dir.join(&proof_file);
    std::fs::write(&proof_path, &proof_bytes)
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", proof_path.display()));
    println!(
        "[test] Wrote FRI fixture for batch {} to {} ({} bytes)",
        batch.number,
        proof_path.display(),
        proof_bytes.len()
    );

    FriProofFixtureManifestEntry {
        batch_number: batch.number,
        batch_file: batch.filename.clone(),
        proof_file,
        proof_bytes: proof_bytes.len(),
        expected_public_input: batch.expected_public_input,
    }
}

/// Writes the manifest atomically: serialize, write to a sibling tmp file,
/// then rename onto the final path. The fixture generator calls this after
/// every successful batch so a crash mid-run still leaves a valid manifest
/// describing the fixtures that did finish — the snark-only replay test can
/// then pick up the partial fixture set without manual recovery.
fn write_fri_fixture_manifest(dir: &std::path::Path, entries: &[FriProofFixtureManifestEntry]) {
    let manifest = FriProofFixtureManifest {
        batches: entries.to_vec(),
    };
    let manifest_path = fri_fixture_manifest_path(dir);
    let tmp_path = manifest_path.with_extension("json.tmp");
    let encoded =
        serde_json::to_vec_pretty(&manifest).expect("failed to serialize FRI fixture manifest");
    std::fs::write(&tmp_path, encoded)
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", tmp_path.display()));
    std::fs::rename(&tmp_path, &manifest_path).unwrap_or_else(|err| {
        panic!(
            "failed to rename {} -> {}: {err}",
            tmp_path.display(),
            manifest_path.display()
        )
    });
    println!(
        "[test] Wrote FRI fixture manifest ({} batch{}): {}",
        entries.len(),
        if entries.len() == 1 { "" } else { "es" },
        manifest_path.display()
    );
}

fn load_fri_fixtures(dir: &std::path::Path) -> Vec<FriProofFixture> {
    let manifest_path = fri_fixture_manifest_path(dir);
    let manifest_bytes = std::fs::read(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));
    let manifest: FriProofFixtureManifest = serde_json::from_slice(&manifest_bytes)
        .unwrap_or_else(|err| panic!("failed to parse {}: {err}", manifest_path.display()));
    assert!(
        !manifest.batches.is_empty(),
        "FRI fixture manifest did not contain any batches"
    );

    manifest
        .batches
        .into_iter()
        .map(|entry| {
            let proof_path = dir.join(&entry.proof_file);
            let proof_bytes = std::fs::read(&proof_path)
                .unwrap_or_else(|err| panic!("failed to read {}: {err}", proof_path.display()));
            assert_eq!(
                proof_bytes.len(),
                entry.proof_bytes,
                "FRI fixture byte length mismatch for {}",
                proof_path.display()
            );
            FriProofFixture {
                batch_number: entry.batch_number,
                batch_file: entry.batch_file,
                proof_bytes,
                expected_public_input: entry.expected_public_input,
            }
        })
        .collect()
}

/// A repository-owned batch input file: its logical batch number and path.
/// (Test-local; the production binary never loads the on-disk batch corpus.)
struct BatchInputFile {
    number: u64,
    path: PathBuf,
}

/// Load and deserialize an `AirbenderVerifierInput` from a batch file.
///
/// Inputs are stored as hex-encoded framed bytes — first 4 bytes (big-endian
/// `u32`) are the bincode payload length, followed by the payload padded out to
/// a multiple of 4 bytes. Files may be plain `.bin` or gzipped `.bin.gz`
/// (tracked in Git LFS).
fn load_batch(batch_input: &BatchInputFile) -> Result<AirbenderVerifierInput> {
    let raw = read_batch_text(&batch_input.path)
        .with_context(|| format!("while attempting to read {}", batch_input.path.display()))?;
    let mut bytes = parse_hex_bytes(&raw).with_context(|| {
        format!(
            "while attempting to parse hex bytes for batch {} from {}",
            batch_input.number,
            batch_input.path.display()
        )
    })?;
    anyhow::ensure!(
        bytes.len() >= 4,
        "batch {}: framed payload too short ({} bytes)",
        batch_input.number,
        bytes.len()
    );
    let byte_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    bytes.drain(..4);
    anyhow::ensure!(
        bytes.len() >= byte_len,
        "batch {}: declared length {byte_len} exceeds available bytes {}",
        batch_input.number,
        bytes.len()
    );
    bytes.truncate(byte_len);

    let (input, decoded_len): (AirbenderVerifierInput, usize) =
        bincode::serde::decode_from_slice(&bytes, bincode::config::standard())
            .with_context(|| format!("while decoding batch {} as bincode", batch_input.number))?;
    anyhow::ensure!(
        decoded_len == bytes.len(),
        "batch {}: trailing bytes after bincode decode ({decoded_len} of {})",
        batch_input.number,
        bytes.len(),
    );
    Ok(input)
}

fn read_batch_text(batch_path: &Path) -> Result<String> {
    match batch_path.extension().and_then(|ext| ext.to_str()) {
        Some("bin") => std::fs::read_to_string(batch_path)
            .with_context(|| format!("while attempting to read {}", batch_path.display())),
        Some("gz") => read_gzip_batch_text(batch_path),
        _ => anyhow::bail!(
            "unsupported batch input path {}, expected a .bin or .bin.gz file",
            batch_path.display()
        ),
    }
}

fn read_gzip_batch_text(batch_path: &Path) -> Result<String> {
    let compressed_bytes = std::fs::read(batch_path)
        .with_context(|| format!("while attempting to read {}", batch_path.display()))?;

    const GIT_LFS_POINTER_PREFIX: &str = "version https://git-lfs.github.com/spec/v1";
    if compressed_bytes.starts_with(GIT_LFS_POINTER_PREFIX.as_bytes()) {
        anyhow::bail!(
            "{} is still a Git LFS pointer; pull the matching LFS object before retrying",
            batch_path.display()
        );
    }

    let mut decoder = GzDecoder::new(compressed_bytes.as_slice());
    let mut raw = String::new();
    decoder.read_to_string(&mut raw).with_context(|| {
        format!(
            "while attempting to decompress UTF-8 text from {}",
            batch_path.display()
        )
    })?;
    Ok(raw)
}

fn parse_hex_bytes(raw: &str) -> Result<Vec<u8>> {
    let mut compact: String = raw.chars().filter(|ch| !ch.is_whitespace()).collect();
    if let Some(stripped) = compact.strip_prefix("0x") {
        compact = stripped.to_owned();
    }

    anyhow::ensure!(!compact.is_empty(), "batch payload is empty");
    // Files are stored in 8-hex-char (= 4-byte) words; require alignment so we
    // catch truncated files early.
    anyhow::ensure!(
        compact.len().is_multiple_of(8),
        "batch payload length must be a multiple of 8 hex characters (got {})",
        compact.len()
    );

    let mut bytes = Vec::with_capacity(compact.len() / 2);
    for chunk in compact.as_bytes().chunks(2) {
        let s = std::str::from_utf8(chunk).context("while decoding hex chunk as UTF-8")?;
        let byte =
            u8::from_str_radix(s, 16).with_context(|| format!("while parsing hex byte `{s}`"))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

/// Loads a batch from the LFS corpus and returns the verifier input plus the
/// natively-computed proof public input that a real proof must match.
fn load_batch_and_expected_public_input(filename: &str) -> BatchTestInput {
    let batch_path = batch_file_path(filename);
    let number = batch_number_from_filename(filename);
    println!("[test] Loading batch from: {}", batch_path.display());
    let batch_input = BatchInputFile {
        number: number.into(),
        path: batch_path,
    };
    let verifier_input = load_batch(&batch_input).expect("failed to load batch");
    let expected_public_input = verifier_input
        .clone()
        .verify()
        .expect("native verify failed")
        .proof_public_input;
    println!(
        "[test] Native verify produced public input for batch {number}: {expected_public_input:?}"
    );
    BatchTestInput {
        number,
        filename: filename.to_owned(),
        verifier_input,
        expected_public_input,
    }
}

/// Verifies a bincode-encoded `Proof` payload against the committed FRI VK.
fn verify_fri_proof(
    proof_bytes: &[u8],
    expected_public_input: &[u32; 8],
    dist_dir: &std::path::Path,
    vk_path: &std::path::Path,
) {
    println!("[test] Building verifier from committed app.bin...");
    let verifier =
        RealVerifierBuilder::new(dist_dir.join("app.bin"), ProverLevel::RecursionUnified)
            .build()
            .expect("failed to build RealVerifier");

    let vk = load_vk_from_disk(vk_path, SecurityLevel::default())
        .expect("failed to load committed FRI verification key");

    println!("[test] Deserializing proof...");
    let (proof, _): (Proof, usize) =
        bincode::serde::decode_from_slice(proof_bytes, bincode::config::standard())
            .expect("failed to deserialize proof bytes");

    println!("[test] Verifying proof...");
    verifier
        .verify(
            &proof,
            &vk,
            VerificationRequest::real(expected_public_input),
        )
        .expect("proof verification failed");

    println!("[test] FRI proof verified successfully!");
}

/// Awaits a `oneshot::Receiver` with a timeout, printing a heartbeat every
/// `HEARTBEAT_INTERVAL`. Used to wait for proof submissions without going
/// silent for many minutes.
async fn await_with_heartbeat(
    label: impl Into<String>,
    receiver: oneshot::Receiver<Vec<u8>>,
    timeout: Duration,
) -> Vec<u8> {
    let label = label.into();
    let started_at = Instant::now();
    tokio::time::timeout(timeout, async {
        let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
        interval.tick().await; // first tick fires immediately, skip it
        let mut receiver = std::pin::pin!(receiver);
        loop {
            tokio::select! {
                result = &mut receiver => {
                    return result.expect("proof channel closed without receiving a proof");
                }
                _ = interval.tick() => {
                    println!(
                        "[test] Still waiting for {label} proof... elapsed: {:.0}s",
                        started_at.elapsed().as_secs_f64()
                    );
                }
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {label} proof"))
}

// ---------------------------------------------------------------------------
// The integration tests
// ---------------------------------------------------------------------------

/// Exercises `fri-only` followed by `snark-only` end-to-end against a single
/// test HTTP server:
/// 1. Start a prover in `fri-only` mode, wait for the FRI submission to land
///    on `/airbender/submit_proofs`, then kill it (the FRI prover claims
///    nearly the entire GPU at init, so the SNARK wrapper can't share).
/// 2. Start a fresh prover in `snark-only` mode. The server hands the just-
///    captured FRI proof back via `/airbender/snark_inputs`, the prover wraps
///    it, and submits the SNARK proof to `/airbender/submit_snark_proofs`.
///
/// The FRI proof is verified cryptographically. The SNARK proof is checked
/// for payload shape only (round-trips through `serde_json` as
/// `SnarkWrapperProof`) since the server crate does not link a SNARK verifier.
#[ignore = "requires GPU, built guest binary, and LFS batch 506093.bin.gz"]
#[tokio::test(flavor = "multi_thread")]
async fn prover_server_proves_fri_then_snark() {
    let dist_dir = guest_dist_dir();
    println!("[test] Guest dist dir: {}", dist_dir.display());

    let trusted_setup = snark_trusted_setup_path();
    println!("[test] SNARK trusted setup: {}", trusted_setup.display());

    let BatchTestInput {
        verifier_input,
        expected_public_input,
        ..
    } = load_batch_and_expected_public_input("506093.bin.gz");

    // --- 2. Set up test HTTP server with all four prover endpoints. The same
    //        server instance is shared by both prover invocations. ---
    let (fri_tx, fri_rx) = oneshot::channel::<Vec<u8>>();
    let (snark_tx, snark_rx) = oneshot::channel::<Vec<u8>>();
    let state = TestServerState {
        verifier_input: Arc::new(verifier_input),
        fri_input_served: Arc::new(AtomicBool::new(false)),
        fri_proof_capture: Arc::new(Mutex::new(None)),
        fri_proof_sender: Arc::new(Mutex::new(Some(fri_tx))),
        snark_input_served: Arc::new(AtomicBool::new(false)),
        snark_proof_sender: Arc::new(Mutex::new(Some(snark_tx))),
    };

    let app = Router::new()
        .route("/airbender/proof_inputs", post(handle_proof_inputs))
        .route(
            "/airbender/submit_proofs",
            post(handle_submit_proofs).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/airbender/snark_inputs",
            post(handle_snark_inputs).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/airbender/submit_snark_proofs",
            post(handle_submit_snark_proofs).layer(DefaultBodyLimit::disable()),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test HTTP server");
    let server_addr = listener
        .local_addr()
        .expect("failed to get test server address");
    println!("[test] Test HTTP server listening on http://{server_addr}");

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("test HTTP server exited with error");
    });

    let prover_bin = prover_server_bin();
    let server_url = format!("http://{server_addr}");
    let dist_dir_str = dist_dir
        .to_str()
        .expect("non-UTF8 guest dist dir")
        .to_owned();
    let trusted_setup_str = trusted_setup
        .to_str()
        .expect("non-UTF8 SNARK trusted setup path")
        .to_owned();
    let fri_vk = fri_vk_path();
    let snark_vk = snark_vk_path();
    assert!(
        fri_vk.exists(),
        "FRI verification key not found at {}. Run `cargo run -p eravm-prover-host -- gen-vks` first.",
        fri_vk.display()
    );
    assert!(
        snark_vk.exists(),
        "SNARK verification key not found at {}. Run `cargo run -p eravm-prover-host -- gen-vks` first.",
        snark_vk.display()
    );
    let fri_vk_str = fri_vk.to_str().expect("non-UTF8 FRI VK path").to_owned();
    let snark_vk_str = snark_vk
        .to_str()
        .expect("non-UTF8 SNARK VK path")
        .to_owned();
    println!("[test] FRI VK: {fri_vk_str}");
    println!("[test] SNARK VK: {snark_vk_str}");

    // --- 3a. Phase 1: spawn fri-only prover, wait for FRI proof, then kill it. ---
    println!(
        "[test] Phase 1: spawning fri-only prover: {}",
        prover_bin.display()
    );
    let fri_child = Command::new(&prover_bin)
        .env("PROVER_SERVER_URL", &server_url)
        .env("PROVER_GUEST_DIST_DIR", &dist_dir_str)
        .env("PROVER_MODE", "fri-only")
        .env("FRI_VK", &fri_vk_str)
        .env("PROVER_POLL_INTERVAL_MS", "1000")
        .env("PROVER_ID", "integration-test-fri")
        .spawn()
        .expect("failed to spawn fri-only eravm-prover-server");
    println!("[test] fri-only prover spawned (pid {})", fri_child.id());
    let fri_guard = ChildGuard(fri_child);

    let fri_timeout = fri_proof_timeout();
    eprintln!(
        "[test] Waiting for FRI proof (timeout: {}s)...",
        fri_timeout.as_secs()
    );
    let started_at = Instant::now();
    let fri_bytes = await_with_heartbeat("FRI", fri_rx, fri_timeout).await;
    println!(
        "[test] FRI proof received after {:.1}s ({} bytes)",
        started_at.elapsed().as_secs_f64(),
        fri_bytes.len()
    );

    // Kill the fri-only prover before starting the snark-only one so they
    // don't fight over the GPU.
    println!("[test] Phase 1 complete; stopping fri-only prover");
    drop(fri_guard);

    // SIGKILL is synchronous from the kernel's PoV, but the CUDA driver
    // doesn't always reap the dead context immediately — kicking off the
    // snark-only prover too soon makes its first `cudaMalloc` race with the
    // tail of the reclaim and fail with `cudaErrorMemoryAllocation`.
    println!(
        "[test] Sleeping {:?} to let the CUDA driver reclaim GPU state",
        GPU_RECLAIM_DELAY
    );
    tokio::time::sleep(GPU_RECLAIM_DELAY).await;

    // --- 3b. Phase 2: spawn snark-only prover. It will poll
    //        `/airbender/snark_inputs`, receive the captured FRI proof, wrap
    //        it, and submit the SNARK proof. ---
    println!(
        "[test] Phase 2: spawning snark-only prover: {}",
        prover_bin.display()
    );
    let snark_child = Command::new(&prover_bin)
        .env("PROVER_SERVER_URL", &server_url)
        .env("PROVER_GUEST_DIST_DIR", &dist_dir_str)
        .env("PROVER_MODE", "snark-only")
        .env("SNARK_TRUSTED_SETUP_FILE", &trusted_setup_str)
        .env("FRI_VK", &fri_vk_str)
        .env("SNARK_VK", &snark_vk_str)
        .env("PROVER_POLL_INTERVAL_MS", "1000")
        .env("PROVER_ID", "integration-test-snark")
        .spawn()
        .expect("failed to spawn snark-only eravm-prover-server");
    println!(
        "[test] snark-only prover spawned (pid {})",
        snark_child.id()
    );
    let _snark_guard = ChildGuard(snark_child);

    let snark_timeout = snark_proof_timeout();
    eprintln!(
        "[test] Waiting for SNARK proof (timeout: {}s)...",
        snark_timeout.as_secs()
    );
    let snark_started_at = Instant::now();
    let snark_bytes = await_with_heartbeat("SNARK", snark_rx, snark_timeout).await;
    println!(
        "[test] SNARK proof received after {:.1}s ({} bytes)",
        snark_started_at.elapsed().as_secs_f64(),
        snark_bytes.len()
    );

    // --- 4. Verify the FRI proof cryptographically and round-trip the SNARK payload. ---
    verify_fri_proof(&fri_bytes, &expected_public_input, &dist_dir, &fri_vk);

    // Round-trip the SNARK payload as a shape check.
    let _snark_proof: SnarkWrapperProof = serde_json::from_slice(&snark_bytes)
        .expect("SNARK proof body did not deserialize as SnarkWrapperProof JSON");
    println!("[test] SNARK proof payload deserialized successfully!");
}

/// Generates reusable bincode-encoded FRI proof fixtures for `snark-only`
/// sizing runs. Set `IT_GENERATE_FRI_FIXTURES=1` and optionally
/// `IT_FRI_FIXTURES_DIR=/path/to/fixtures`.
///
/// Reuses a single long-lived `fri-only` prover across all batches: the test
/// server serves them in order via `/airbender/proof_inputs`, handing out
/// batch N only once batch N-1's FRI proof has landed. This exercises the same
/// back-to-back proving path as the live multi-batch test — historically it
/// tripped a state-bleed bug in `gpu_prover::ExecutionProver`'s pooled
/// `LockedBoxedMemoryHolder`/`LockedBoxedTraceChunk` buffers (stale memory caps
/// from a prior simulation leaking into the next, surfacing as an
/// `assert_caps_mach` panic in `commit_memory_and_prove` around the third
/// batch). If that resurfaces, fall back to one prover per batch.
#[ignore = "requires GPU, built guest binary, and LFS batches"]
#[tokio::test(flavor = "multi_thread")]
async fn prover_server_generates_fri_fixtures() {
    if !truthy_env_var("IT_GENERATE_FRI_FIXTURES") {
        println!(
            "[test] Skipping FRI fixture generation; set IT_GENERATE_FRI_FIXTURES=1 to enable"
        );
        return;
    }

    let dist_dir = guest_dist_dir();
    println!("[test] Guest dist dir: {}", dist_dir.display());

    let fixture_dir = fri_fixtures_dir();
    println!("[test] FRI fixture dir: {}", fixture_dir.display());

    let fri_vk = fri_vk_path();
    assert!(
        fri_vk.exists(),
        "FRI verification key not found at {}. Run `cargo run -p eravm-prover-host -- gen-vks` first.",
        fri_vk.display()
    );
    let fri_vk_str = fri_vk.to_str().expect("non-UTF8 FRI VK path").to_owned();
    println!("[test] FRI VK: {fri_vk_str}");

    let batch_files = batch_files_from_env(&["506093.bin.gz", "506094.bin.gz", "506095.bin.gz"]);
    println!("[test] FRI fixture batch files: {batch_files:?}");
    let batches: Vec<_> = batch_files
        .iter()
        .map(|filename| load_batch_and_expected_public_input(filename))
        .collect();
    let batches = Arc::new(batches);

    let mut fri_proof_senders = HashMap::new();
    let mut receivers = Vec::new();
    for batch in batches.iter() {
        let (fri_tx, fri_rx) = oneshot::channel::<Vec<u8>>();
        if fri_proof_senders.insert(batch.number, fri_tx).is_some() {
            panic!("duplicate batch number in IT_BATCH_FILES: {}", batch.number);
        }
        receivers.push((batch.number, fri_rx));
    }

    let state = FriFixtureGenerationState {
        batches: Arc::clone(&batches),
        next_fri_index: Arc::new(Mutex::new(0)),
        completed_fri_count: Arc::new(AtomicUsize::new(0)),
        fri_proof_senders: Arc::new(Mutex::new(fri_proof_senders)),
    };

    let app = Router::new()
        .route(
            "/airbender/proof_inputs",
            post(handle_fri_fixture_proof_inputs),
        )
        .route(
            "/airbender/submit_proofs",
            post(handle_fri_fixture_submit_proofs).layer(DefaultBodyLimit::disable()),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test HTTP server");
    let server_addr = listener
        .local_addr()
        .expect("failed to get test server address");
    println!("[test] Test HTTP server listening on http://{server_addr}");

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("test HTTP server exited with error");
    });

    let prover_bin = prover_server_bin();
    let server_url = format!("http://{server_addr}");
    let dist_dir_str = dist_dir
        .to_str()
        .expect("non-UTF8 guest dist dir")
        .to_owned();

    println!(
        "[test] Spawning one long-lived fri-only prover for {} fixture batches: {}",
        batches.len(),
        prover_bin.display()
    );

    // One long-lived prover services every batch. The server gates
    // `/airbender/proof_inputs` so it only sees batch N after batch N-1's FRI
    // proof lands, but the GPU context persists across all of them.
    let child = Command::new(&prover_bin)
        .env("PROVER_SERVER_URL", &server_url)
        .env("PROVER_GUEST_DIST_DIR", &dist_dir_str)
        .env("PROVER_MODE", "fri-only")
        .env("FRI_VK", &fri_vk_str)
        .env("PROVER_POLL_INTERVAL_MS", "1000")
        .env("PROVER_ID", "integration-test-fri-fixtures")
        .spawn()
        .expect("failed to spawn fri-only eravm-prover-server");
    println!("[test] fri-only prover spawned (pid {})", child.id());
    let _guard = ChildGuard(child);

    let mut manifest_entries = Vec::new();
    let full_run_started_at = Instant::now();
    let fri_timeout = fri_proof_timeout();
    for (batch, (batch_number, fri_rx)) in batches.iter().zip(receivers) {
        assert_eq!(batch.number, batch_number);
        let batch_started_at = Instant::now();

        eprintln!(
            "[test] Waiting for FRI fixture proof for batch {} (timeout: {}s)...",
            batch.number,
            fri_timeout.as_secs()
        );
        let fri_bytes =
            await_with_heartbeat(format!("FRI batch {}", batch.number), fri_rx, fri_timeout).await;
        println!(
            "[test] FRI fixture proof for batch {} received after {:.1}s ({} bytes)",
            batch.number,
            batch_started_at.elapsed().as_secs_f64(),
            fri_bytes.len()
        );

        verify_fri_proof(&fri_bytes, &batch.expected_public_input, &dist_dir, &fri_vk);
        manifest_entries.push(write_fri_fixture(&fixture_dir, batch, fri_bytes));
        // Rewrite the manifest after each batch so a later panic (e.g. the
        // `assert_caps_mach` state-bleed bug resurfacing around batch 3) still
        // leaves a valid manifest covering the batches that did finish.
        write_fri_fixture_manifest(&fixture_dir, &manifest_entries);
        println!(
            "[test] Batch {} FRI fixture finished in {:.1}s",
            batch.number,
            batch_started_at.elapsed().as_secs_f64()
        );
    }

    println!(
        "[test] FRI fixture generation finished in {:.1}s",
        full_run_started_at.elapsed().as_secs_f64()
    );
}

/// Replays existing FRI proof fixtures through one live `snark-only` prover.
/// This isolates SNARK wrapper sizing from the FRI prover's resident GPU
/// allocator. Set `IT_RUN_SNARK_ONLY_REPLAY=1` and `IT_FRI_FIXTURES_DIR`.
#[ignore = "requires GPU, built guest binary, FRI fixtures, and trusted setup"]
#[tokio::test(flavor = "multi_thread")]
async fn prover_server_replays_snark_only_fixtures() {
    if !truthy_env_var("IT_RUN_SNARK_ONLY_REPLAY") {
        println!(
            "[test] Skipping snark-only fixture replay; set IT_RUN_SNARK_ONLY_REPLAY=1 to enable"
        );
        return;
    }

    let dist_dir = guest_dist_dir();
    println!("[test] Guest dist dir: {}", dist_dir.display());

    let trusted_setup = snark_trusted_setup_path();
    println!("[test] SNARK trusted setup: {}", trusted_setup.display());

    let fri_vk = fri_vk_path();
    let snark_vk = snark_vk_path();
    assert!(
        fri_vk.exists(),
        "FRI verification key not found at {}. Run `cargo run -p eravm-prover-host -- gen-vks` first.",
        fri_vk.display()
    );
    assert!(
        snark_vk.exists(),
        "SNARK verification key not found at {}. Run `cargo run -p eravm-prover-host -- gen-vks` first.",
        snark_vk.display()
    );
    let fri_vk_str = fri_vk.to_str().expect("non-UTF8 FRI VK path").to_owned();
    let snark_vk_str = snark_vk
        .to_str()
        .expect("non-UTF8 SNARK VK path")
        .to_owned();
    println!("[test] FRI VK: {fri_vk_str}");
    println!("[test] SNARK VK: {snark_vk_str}");

    let fixture_dir = fri_fixtures_dir();
    println!("[test] FRI fixture dir: {}", fixture_dir.display());
    let fixtures = load_fri_fixtures(&fixture_dir);
    println!("[test] Loaded {} FRI fixtures", fixtures.len());
    for fixture in &fixtures {
        println!(
            "[test] Fixture batch {} ({}) has {} bytes",
            fixture.batch_number,
            fixture.batch_file,
            fixture.proof_bytes.len()
        );
        verify_fri_proof(
            &fixture.proof_bytes,
            &fixture.expected_public_input,
            &dist_dir,
            &fri_vk,
        );
    }
    let fixtures = Arc::new(fixtures);

    let mut snark_proof_senders = HashMap::new();
    let mut receivers = Vec::new();
    for fixture in fixtures.iter() {
        let (snark_tx, snark_rx) = oneshot::channel::<Vec<u8>>();
        if snark_proof_senders
            .insert(fixture.batch_number, snark_tx)
            .is_some()
        {
            panic!(
                "duplicate batch number in FRI fixtures: {}",
                fixture.batch_number
            );
        }
        receivers.push((fixture.batch_number, snark_rx));
    }

    let state = SnarkFixtureReplayState {
        fixtures: Arc::clone(&fixtures),
        next_snark_index: Arc::new(Mutex::new(0)),
        completed_snark_count: Arc::new(AtomicUsize::new(0)),
        snark_proof_senders: Arc::new(Mutex::new(snark_proof_senders)),
    };

    let app = Router::new()
        .route(
            "/airbender/snark_inputs",
            post(handle_fixture_snark_inputs).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/airbender/submit_snark_proofs",
            post(handle_fixture_submit_snark_proofs).layer(DefaultBodyLimit::disable()),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test HTTP server");
    let server_addr = listener
        .local_addr()
        .expect("failed to get test server address");
    println!("[test] Test HTTP server listening on http://{server_addr}");

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("test HTTP server exited with error");
    });

    let prover_bin = prover_server_bin();
    let server_url = format!("http://{server_addr}");
    let dist_dir_str = dist_dir
        .to_str()
        .expect("non-UTF8 guest dist dir")
        .to_owned();
    let trusted_setup_str = trusted_setup
        .to_str()
        .expect("non-UTF8 SNARK trusted setup path")
        .to_owned();

    println!(
        "[test] Spawning one snark-only prover for {} fixture batches: {}",
        fixtures.len(),
        prover_bin.display()
    );
    let child = Command::new(&prover_bin)
        .env("PROVER_SERVER_URL", &server_url)
        .env("PROVER_GUEST_DIST_DIR", &dist_dir_str)
        .env("PROVER_MODE", "snark-only")
        .env("SNARK_TRUSTED_SETUP_FILE", &trusted_setup_str)
        .env("FRI_VK", &fri_vk_str)
        .env("SNARK_VK", &snark_vk_str)
        .env("PROVER_POLL_INTERVAL_MS", "1000")
        .env("PROVER_ID", "integration-test-snark-fixtures")
        .spawn()
        .expect("failed to spawn snark-only eravm-prover-server");
    println!("[test] snark-only prover spawned (pid {})", child.id());
    let _guard = ChildGuard(child);

    let full_run_started_at = Instant::now();
    let snark_timeout = snark_proof_timeout();
    for (fixture, (batch_number, snark_rx)) in fixtures.iter().zip(receivers) {
        assert_eq!(fixture.batch_number, batch_number);
        let batch_started_at = Instant::now();

        eprintln!(
            "[test] Waiting for SNARK fixture proof for batch {} (timeout: {}s)...",
            fixture.batch_number,
            snark_timeout.as_secs()
        );
        let snark_bytes = await_with_heartbeat(
            format!("SNARK batch {}", fixture.batch_number),
            snark_rx,
            snark_timeout,
        )
        .await;
        println!(
            "[test] SNARK fixture proof for batch {} received after {:.1}s ({} bytes)",
            fixture.batch_number,
            batch_started_at.elapsed().as_secs_f64(),
            snark_bytes.len()
        );

        let _snark_proof: SnarkWrapperProof = serde_json::from_slice(&snark_bytes)
            .expect("SNARK proof body did not deserialize as SnarkWrapperProof JSON");
        println!(
            "[test] Batch {} snark-only fixture replay finished in {:.1}s",
            fixture.batch_number,
            batch_started_at.elapsed().as_secs_f64()
        );
    }

    println!(
        "[test] Snark-only fixture replay finished in {:.1}s",
        full_run_started_at.elapsed().as_secs_f64()
    );
}

/// Exercises one live `fri-snark` prover across several batches. This is the
/// stricter memory-retention path: no process restart, no explicit CUDA
/// reclaim, and no switch between `fri-only` and `snark-only`.
///
/// This is a manual Vast sizing test. Set `IT_RUN_MULTI_BATCH_FRI_SNARK=1` to
/// opt in; normal CI runs all ignored tests and should not run this stress path.
///
/// Override the batch list with `IT_BATCH_FILES=506093.bin.gz,506094.bin.gz`.
#[ignore = "requires GPU, built guest binary, and LFS batches"]
#[tokio::test(flavor = "multi_thread")]
async fn prover_server_proves_multi_batch_fri_snark() {
    if !truthy_env_var("IT_RUN_MULTI_BATCH_FRI_SNARK") {
        println!(
            "[test] Skipping manual multi-batch fri-snark test; set IT_RUN_MULTI_BATCH_FRI_SNARK=1 to enable"
        );
        return;
    }

    let dist_dir = guest_dist_dir();
    println!("[test] Guest dist dir: {}", dist_dir.display());

    let trusted_setup = snark_trusted_setup_path();
    println!("[test] SNARK trusted setup: {}", trusted_setup.display());

    let fri_vk = fri_vk_path();
    let snark_vk = snark_vk_path();
    assert!(
        fri_vk.exists(),
        "FRI verification key not found at {}. Run `cargo run -p eravm-prover-host -- gen-vks` first.",
        fri_vk.display()
    );
    assert!(
        snark_vk.exists(),
        "SNARK verification key not found at {}. Run `cargo run -p eravm-prover-host -- gen-vks` first.",
        snark_vk.display()
    );
    let fri_vk_str = fri_vk.to_str().expect("non-UTF8 FRI VK path").to_owned();
    let snark_vk_str = snark_vk
        .to_str()
        .expect("non-UTF8 SNARK VK path")
        .to_owned();
    println!("[test] FRI VK: {fri_vk_str}");
    println!("[test] SNARK VK: {snark_vk_str}");

    let batch_files = batch_files_from_env(&["506093.bin.gz", "506094.bin.gz", "506095.bin.gz"]);
    println!("[test] Multi-batch fri-snark files: {batch_files:?}");
    let batches: Vec<_> = batch_files
        .iter()
        .map(|filename| load_batch_and_expected_public_input(filename))
        .collect();
    let batches = Arc::new(batches);

    let mut fri_proof_senders = HashMap::new();
    let mut snark_proof_senders = HashMap::new();
    let mut receivers = Vec::new();
    for batch in batches.iter() {
        let (fri_tx, fri_rx) = oneshot::channel::<Vec<u8>>();
        let (snark_tx, snark_rx) = oneshot::channel::<Vec<u8>>();
        if fri_proof_senders.insert(batch.number, fri_tx).is_some() {
            panic!("duplicate batch number in IT_BATCH_FILES: {}", batch.number);
        }
        if snark_proof_senders.insert(batch.number, snark_tx).is_some() {
            panic!("duplicate batch number in IT_BATCH_FILES: {}", batch.number);
        }
        receivers.push((batch.number, fri_rx, snark_rx));
    }

    let state = MultiBatchTestServerState {
        batches: Arc::clone(&batches),
        next_fri_index: Arc::new(Mutex::new(0)),
        completed_snark_count: Arc::new(AtomicUsize::new(0)),
        fri_proof_senders: Arc::new(Mutex::new(fri_proof_senders)),
        snark_proof_senders: Arc::new(Mutex::new(snark_proof_senders)),
    };

    let app = Router::new()
        .route(
            "/airbender/proof_inputs",
            post(handle_multi_batch_proof_inputs),
        )
        .route(
            "/airbender/submit_proofs",
            post(handle_multi_batch_submit_proofs).layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/airbender/submit_snark_proofs",
            post(handle_multi_batch_submit_snark_proofs).layer(DefaultBodyLimit::disable()),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test HTTP server");
    let server_addr = listener
        .local_addr()
        .expect("failed to get test server address");
    println!("[test] Test HTTP server listening on http://{server_addr}");

    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("test HTTP server exited with error");
    });

    let prover_bin = prover_server_bin();
    let server_url = format!("http://{server_addr}");
    let dist_dir_str = dist_dir
        .to_str()
        .expect("non-UTF8 guest dist dir")
        .to_owned();
    let trusted_setup_str = trusted_setup
        .to_str()
        .expect("non-UTF8 SNARK trusted setup path")
        .to_owned();

    println!(
        "[test] Spawning one fri-snark prover for {} batches: {}",
        batches.len(),
        prover_bin.display()
    );
    let child = Command::new(&prover_bin)
        .env("PROVER_SERVER_URL", &server_url)
        .env("PROVER_GUEST_DIST_DIR", &dist_dir_str)
        .env("PROVER_MODE", "fri-snark")
        .env("SNARK_TRUSTED_SETUP_FILE", &trusted_setup_str)
        .env("FRI_VK", &fri_vk_str)
        .env("SNARK_VK", &snark_vk_str)
        .env("PROVER_POLL_INTERVAL_MS", "1000")
        .env("PROVER_ID", "integration-test-fri-snark")
        .spawn()
        .expect("failed to spawn fri-snark eravm-prover-server");
    println!("[test] fri-snark prover spawned (pid {})", child.id());
    let _guard = ChildGuard(child);

    let full_run_started_at = Instant::now();
    let fri_timeout = fri_proof_timeout();
    let snark_timeout = snark_proof_timeout();
    for (batch, (batch_number, fri_rx, snark_rx)) in batches.iter().zip(receivers) {
        assert_eq!(batch.number, batch_number);
        let batch_started_at = Instant::now();

        eprintln!(
            "[test] Waiting for FRI proof for batch {} (timeout: {}s)...",
            batch.number,
            fri_timeout.as_secs()
        );
        let fri_started_at = Instant::now();
        let fri_bytes =
            await_with_heartbeat(format!("FRI batch {}", batch.number), fri_rx, fri_timeout).await;
        println!(
            "[test] FRI proof for batch {} received after {:.1}s ({} bytes)",
            batch.number,
            fri_started_at.elapsed().as_secs_f64(),
            fri_bytes.len()
        );

        eprintln!(
            "[test] Waiting for SNARK proof for batch {} (timeout: {}s)...",
            batch.number,
            snark_timeout.as_secs()
        );
        let snark_started_at = Instant::now();
        let snark_bytes = await_with_heartbeat(
            format!("SNARK batch {}", batch.number),
            snark_rx,
            snark_timeout,
        )
        .await;
        println!(
            "[test] SNARK proof for batch {} received after {:.1}s ({} bytes)",
            batch.number,
            snark_started_at.elapsed().as_secs_f64(),
            snark_bytes.len()
        );

        verify_fri_proof(&fri_bytes, &batch.expected_public_input, &dist_dir, &fri_vk);
        let _snark_proof: SnarkWrapperProof = serde_json::from_slice(&snark_bytes)
            .expect("SNARK proof body did not deserialize as SnarkWrapperProof JSON");
        println!(
            "[test] Batch {} fri-snark sequence finished in {:.1}s",
            batch.number,
            batch_started_at.elapsed().as_secs_f64()
        );
    }

    println!(
        "[test] Multi-batch fri-snark run finished in {:.1}s",
        full_run_started_at.elapsed().as_secs_f64()
    );
}
