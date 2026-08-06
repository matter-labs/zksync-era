use std::time::Duration;

use airbender_host::{Inputs, Proof};
use anyhow::{Context, Result};
use eravm_prover_host::SnarkWrapperProof;
use serde::{de::DeserializeOwned, Serialize};
use tracing::{info, warn};
use zksync_airbender_verifier::types::AirbenderVerifierInput;

use crate::types::{JobOrigin, SubmitFriProofRequest, SubmitSnarkProofRequest, WorkerJob};

const FRI_INPUTS_PATH: &str = "/airbender/proof_inputs";
const SNARK_INPUTS_PATH: &str = "/airbender/snark_inputs";
const SUBMIT_FRI_PATH: &str = "/airbender/submit_proofs";
const SUBMIT_SNARK_PATH: &str = "/airbender/submit_snark_proofs";

const FRI_LABEL: &str = "FRI";
const SNARK_LABEL: &str = "SNARK";

/// Status code + error pair returned by request-level helpers; `None` for
/// transport-level errors that have no server status.
type RequestResult = Result<(), (Option<reqwest::StatusCode>, anyhow::Error)>;

/// Thin HTTP client for one job server (one chain): fetches inputs and
/// submits results. Stateless beyond its configured endpoints and HTTP
/// clients — owns no scheduling, channels, or in-flight job state.
pub struct JobServerClient {
    /// One client for both polling and submitting; the per-request timeout
    /// (`poll_timeout` for fetches, `submit_timeout` for submissions — the
    /// latter is larger for big SNARK payloads) is applied per call.
    client: reqwest::blocking::Client,
    /// Index of this client in the configured job-server list; stamped onto
    /// every fetched job (as [`JobOrigin::server`]) so results route back to
    /// the originating server.
    server_index: usize,
    server_url: String,
    prover_id: String,
    submit_attempts: usize,
    poll_timeout: Duration,
    submit_timeout: Duration,
}

impl JobServerClient {
    pub fn new(
        server_index: usize,
        prover_id: String,
        submit_attempts: usize,
        server_url: String,
        connection_timeout: Duration,
        poll_timeout: Duration,
        submit_timeout: Duration,
    ) -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(connection_timeout)
            .build()
            .context("while building HTTP client")?;
        Ok(Self {
            client,
            server_index,
            server_url,
            prover_id,
            submit_attempts,
            poll_timeout,
            submit_timeout,
        })
    }

    pub fn server_url(&self) -> &str {
        &self.server_url
    }

    pub fn fetch_fri_job(&self) -> Result<Option<WorkerJob>> {
        // The v31 wire format has no version envelope: the payload is the input itself.
        let Some(input) = self.poll_json::<AirbenderVerifierInput>(FRI_INPUTS_PATH, FRI_LABEL)?
        else {
            return Ok(None);
        };
        let batch_number = input.vm_run_data.l1_batch_number.0;
        // The chain id rides inside the input's `system_env` — the input is
        // encoded verbatim into the guest program's words, so the server
        // cannot add a separate top-level field without changing what the
        // (hash-committed) guest sees.
        let chain_id = input.system_env.chain_id.as_u64();
        let mut inputs = Inputs::new();
        inputs
            .push(&input)
            .context("failed to encode AirbenderVerifierInput")?;
        Ok(Some(WorkerJob::Fri {
            origin: JobOrigin {
                server: self.server_index,
                chain_id,
            },
            batch_number,
            input_words: inputs.words().to_vec(),
        }))
    }

    pub fn fetch_snark_job(&self) -> Result<Option<WorkerJob>> {
        #[serde_with::serde_as]
        #[derive(serde::Deserialize)]
        struct SnarkInputBody {
            l1_batch_number: u32,
            /// `Option` + default so job servers predating the field still
            /// parse; they report as chain id 0 in metrics/logs.
            #[serde(default)]
            chain_id: Option<u64>,
            #[serde_as(as = "serde_with::hex::Hex")]
            fri_proof: Vec<u8>,
        }
        let Some(body) = self.poll_json::<SnarkInputBody>(SNARK_INPUTS_PATH, SNARK_LABEL)? else {
            return Ok(None);
        };
        let (proof, len): (Proof, usize) =
            bincode::serde::decode_from_slice(&body.fri_proof, bincode::config::standard())
                .context("failed to bincode-decode incoming FRI proof envelope")?;
        if len != body.fri_proof.len() {
            anyhow::bail!("incoming FRI proof envelope has trailing bytes");
        }
        Ok(Some(WorkerJob::Snark {
            origin: JobOrigin {
                server: self.server_index,
                chain_id: body.chain_id.unwrap_or(0),
            },
            batch_number: body.l1_batch_number,
            proof: Box::new(proof),
        }))
    }

    pub fn submit_fri(
        &self,
        chain_id: u64,
        batch_number: u32,
        proof: &Proof,
        cycles_used: u64,
    ) -> Result<()> {
        let proof_bytes = bincode::serde::encode_to_vec(proof, bincode::config::standard())
            .context("failed to bincode-encode FRI proof")?;
        self.submit_with_retries(FRI_LABEL, chain_id, batch_number, |attempt, attempts| {
            info!(
                chain_id,
                batch_number,
                proof_bytes = proof_bytes.len(),
                cycles_used,
                attempt,
                attempts,
                "Submitting FRI proof"
            );
            let payload = SubmitFriProofRequest {
                l1_batch_number: batch_number,
                prover_id: self.prover_id.clone(),
                proof: Some(&proof_bytes),
                error: None,
                cycles_used: Some(cycles_used),
            };
            self.post_payload(FRI_LABEL, batch_number, SUBMIT_FRI_PATH, &payload)
        })
    }

    pub fn submit_snark(
        &self,
        chain_id: u64,
        batch_number: u32,
        proof: Box<SnarkWrapperProof>,
    ) -> Result<()> {
        self.submit_with_retries(SNARK_LABEL, chain_id, batch_number, |attempt, attempts| {
            info!(
                chain_id,
                batch_number, attempt, attempts, "Submitting SNARK proof"
            );
            let payload = SubmitSnarkProofRequest {
                l1_batch_number: batch_number,
                prover_id: self.prover_id.clone(),
                snark_proof: Some(proof.clone()),
                error: None,
            };
            self.post_payload(SNARK_LABEL, batch_number, SUBMIT_SNARK_PATH, &payload)
        })
    }

    /// Reports a FRI proving failure to the server, releasing the batch for
    /// retry without waiting for the proving timeout. Posts to the same
    /// endpoint as [`Self::submit_fri`] but carries `error` instead of a proof.
    pub fn submit_fri_error(&self, chain_id: u64, batch_number: u32, error: &str) -> Result<()> {
        self.submit_with_retries(FRI_LABEL, chain_id, batch_number, |attempt, attempts| {
            info!(
                chain_id,
                batch_number, attempt, attempts, "Submitting FRI failure"
            );
            let payload = SubmitFriProofRequest {
                l1_batch_number: batch_number,
                prover_id: self.prover_id.clone(),
                proof: None,
                error: Some(error.to_owned()),
                cycles_used: None,
            };
            self.post_payload(FRI_LABEL, batch_number, SUBMIT_FRI_PATH, &payload)
        })
    }

    /// Reports a SNARK proving failure to the server. The server reverts the
    /// batch to `generated`, preserving the FRI proof so only the SNARK stage
    /// is retried.
    pub fn submit_snark_error(&self, chain_id: u64, batch_number: u32, error: &str) -> Result<()> {
        self.submit_with_retries(SNARK_LABEL, chain_id, batch_number, |attempt, attempts| {
            info!(
                chain_id,
                batch_number, attempt, attempts, "Submitting SNARK failure"
            );
            let payload = SubmitSnarkProofRequest {
                l1_batch_number: batch_number,
                prover_id: self.prover_id.clone(),
                snark_proof: None,
                error: Some(error.to_owned()),
            };
            self.post_payload(SNARK_LABEL, batch_number, SUBMIT_SNARK_PATH, &payload)
        })
    }

    /// POSTs to `path` and decodes the JSON body. Returns `None` on 204 No
    /// Content. Unexpected statuses are logged and yield `None` (treated as
    /// "no job available" so the caller backs off).
    fn poll_json<R: DeserializeOwned>(&self, path: &str, label: &str) -> Result<Option<R>> {
        let url = format!("{}{path}", self.server_url);
        let response = self
            .client
            .post(&url)
            .timeout(self.poll_timeout)
            .send()
            .with_context(|| format!("while polling {url}"))?;
        match response.status() {
            reqwest::StatusCode::OK => {
                Ok(Some(response.json::<R>().with_context(|| {
                    format!("while deserializing {label} input")
                })?))
            }
            reqwest::StatusCode::NO_CONTENT => Ok(None),
            status => {
                warn!(%status, label, "Unexpected status from job server");
                Ok(None)
            }
        }
    }

    /// POSTs `payload` as JSON and checks for HTTP success. Returns the status
    /// alongside any error so the caller can decide whether to retry.
    fn post_payload<P: Serialize>(
        &self,
        label: &str,
        batch_number: u32,
        path: &str,
        payload: &P,
    ) -> RequestResult {
        let url = format!("{}{path}", self.server_url);
        let response = self
            .client
            .post(&url)
            .timeout(self.submit_timeout)
            .json(payload)
            .send()
            .with_context(|| format!("while submitting {label} proof to {url}"))
            .map_err(|e| (None, e))?;
        let status = response.status();
        if !status.is_success() {
            return Err((
                Some(status),
                anyhow::anyhow!(
                    "server returned {status} when submitting {label} proof for batch {batch_number}"
                ),
            ));
        }
        Ok(())
    }

    /// Retries `attempt_fn` up to `submit_attempts` times for retriable errors
    /// (transport-level or 429/5xx). Per-attempt warnings are emitted here;
    /// the caller logs the final success/error.
    fn submit_with_retries<F>(
        &self,
        label: &str,
        chain_id: u64,
        batch_number: u32,
        mut attempt_fn: F,
    ) -> Result<()>
    where
        F: FnMut(usize, usize) -> RequestResult,
    {
        let attempts = self.submit_attempts;
        let mut last_err = anyhow::anyhow!("no attempts made");
        for attempt in 1..=attempts {
            match attempt_fn(attempt, attempts) {
                Ok(()) => return Ok(()),
                Err((status, err)) => {
                    let retriable = status.is_none_or(|s| {
                        s == reqwest::StatusCode::TOO_MANY_REQUESTS || s.is_server_error()
                    });
                    if !retriable {
                        return Err(err.context(format!(
                            "non-retriable status submitting {label} proof for batch {batch_number}"
                        )));
                    }
                    warn!(
                        label,
                        chain_id,
                        batch_number,
                        attempt,
                        attempts,
                        ?err,
                        "Submit attempt failed"
                    );
                    last_err = err;
                    if attempt < attempts {
                        // Exponential backoff: 100ms, 200ms, 400ms, … capped at
                        // 5s, so a 5xx storm isn't hammered with near-instant
                        // retries.
                        let backoff = Duration::from_millis(100 * (1u64 << (attempt - 1)))
                            .min(Duration::from_secs(5));
                        std::thread::sleep(backoff);
                    }
                }
            }
        }
        Err(last_err.context(format!(
            "failed to submit {label} proof for batch {batch_number} after {attempts} attempts"
        )))
    }
}
