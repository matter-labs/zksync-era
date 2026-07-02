//! Commitment inputs for the two pieces.
//!
//! `CommitmentInput` carries the *previous* batch's commitment hashes plus
//! *this* batch's blob hashes. Mirrors
//! `airbender_request_processor::airbender_verifier_input_for_existing_batch`
//! (the blob-hash + prev-commitment assembly), specialized per piece:
//!
//! * Piece A: `prev_*` = batch N-1's real Airbender commitment; blob hashes
//!   from A's own pubdata.
//! * Piece B: `prev_*` = **piece A's** synthetic commitment; blob hashes from
//!   B's pubdata. Computing A's commitment is the deepest step and is gated
//!   behind `--commitment-chaining` (see [`commitment_input_for_b`]).

use zksync_airbender_prover_interface::inputs::{BlobHash, CommitmentInput};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_l1_contract_interface::i_executor::commit::kzg::{
    pubdata_to_blob_commitments, pubdata_to_blob_linear_hashes, pubdata_to_blob_versioned_hashes,
};
use zksync_types::{
    blob::num_blobs_required, commitment::L1BatchCommitmentMode, L1BatchNumber, ProtocolVersionId,
    H256,
};

/// Compute a batch's EIP-4844 blob hashes from its pubdata (Validium zeroes
/// them, matching `commitment_generator`).
pub fn blob_hashes(
    pubdata_input: &[u8],
    version: &ProtocolVersionId,
    mode: L1BatchCommitmentMode,
) -> (Vec<BlobHash>, Vec<H256>) {
    let num_blobs = num_blobs_required(version);
    let versioned_hashes = pubdata_to_blob_versioned_hashes(num_blobs, pubdata_input);

    let blob_hashes = match mode {
        L1BatchCommitmentMode::Rollup => {
            let commitments = pubdata_to_blob_commitments(num_blobs, pubdata_input);
            let linear_hashes = pubdata_to_blob_linear_hashes(num_blobs, pubdata_input.to_vec());
            commitments
                .into_iter()
                .zip(linear_hashes)
                .map(|(commitment, linear_hash)| BlobHash {
                    commitment,
                    linear_hash,
                })
                .collect()
        }
        L1BatchCommitmentMode::Validium => vec![BlobHash::default(); num_blobs],
    };
    (blob_hashes, versioned_hashes)
}

/// Piece A starts where batch N does, so its predecessor commitment is batch
/// N-1's — read exactly as the request processor reads it.
pub async fn commitment_input_for_a(
    connection: &mut Connection<'_, Core>,
    batch: L1BatchNumber,
    piece_a_pubdata: &[u8],
    version: &ProtocolVersionId,
    mode: L1BatchCommitmentMode,
) -> anyhow::Result<CommitmentInput> {
    let prev = L1BatchNumber(batch.0.checked_sub(1).context_no_predecessor(batch)?);
    let prev_commitment = connection
        .blocks_dal()
        .get_prev_batch_airbender_commitment_input(prev)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "previous batch {prev} has no Airbender commitment input yet — \
                 commitment_generator must run before splitting"
            )
        })?;

    let (blob_hashes, blob_versioned_hashes) = blob_hashes(piece_a_pubdata, version, mode);

    Ok(CommitmentInput {
        prev_batch_commitment: prev_commitment.prev_batch_commitment,
        prev_meta_hash: prev_commitment.meta_parameters_hash,
        prev_aux_hash: prev_commitment.prev_aux_hash,
        blob_hashes,
        blob_versioned_hashes,
    })
}

/// Piece B's predecessor is the synthetic intermediate batch A.
///
/// When `chaining` is false (the default for geometry-feasibility runs) this
/// returns `None`: the verifier only requires `Some` for *full* proving, and
/// `None` is enough to check that each half fits Airbender's geometry.
///
/// When `chaining` is true, `prev_*` must be **piece A's Airbender commitment**:
/// assemble an `L1BatchCommitment` from A's re-execution output (system logs,
/// state diffs, `AuxCommitments` via `AirbenderCommitmentComputer`, tree data
/// = `root_A` + A's leaf count) and call
/// `zksync_types::commitment::L1BatchCommitment::airbender_artifacts()`.
/// See `core/node/commitment_generator/src/{lib,utils}.rs`. This is the one
/// piece that cannot be lifted verbatim from existing call sites and is left
/// unimplemented until validated end-to-end.
pub fn commitment_input_for_b(
    chaining: bool,
    piece_b_pubdata: &[u8],
    version: &ProtocolVersionId,
    mode: L1BatchCommitmentMode,
) -> anyhow::Result<Option<CommitmentInput>> {
    if !chaining {
        return Ok(None);
    }
    let (_blob_hashes, _blob_versioned_hashes) = blob_hashes(piece_b_pubdata, version, mode);
    anyhow::bail!(
        "--commitment-chaining is not yet implemented: computing piece A's Airbender commitment \
         (prev_* for piece B) requires assembling an L1BatchCommitment from A's outputs. Run \
         without chaining to produce geometry-feasibility pieces (commitment_input = None)."
    )
}

trait NoPredecessor {
    fn context_no_predecessor(self, batch: L1BatchNumber) -> anyhow::Result<u32>;
}
impl NoPredecessor for Option<u32> {
    fn context_no_predecessor(self, batch: L1BatchNumber) -> anyhow::Result<u32> {
        self.ok_or_else(|| {
            anyhow::anyhow!("batch {batch} has no predecessor (only batch 1+ are splittable)")
        })
    }
}
