//! Object-store wrapper for the two split pieces.
//!
//! `AirbenderVerifierInput` is normally assembled on the fly by the
//! proof-data-handler and never persisted. The split pieces (especially B,
//! which has no canonical DB row) must be self-contained, so we persist each as
//! a CBOR `AirbenderVerifierInput` under a dedicated sub-key.

use zksync_airbender_prover_interface::inputs::AirbenderVerifierInput;
use zksync_object_store::{Bucket, StoredObject, _reexports::BoxedError};
use zksync_types::L1BatchNumber;

/// Which half of the split a blob holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    /// Blocks `[first, mid)`, proving `old_root_N → root_A`.
    A,
    /// Blocks `[mid, last]`, proving `root_A → new_root_N`.
    B,
}

impl Half {
    fn suffix(self) -> &'static str {
        match self {
            Half::A => "a",
            Half::B => "b",
        }
    }
}

/// CBOR-serialized split piece, keyed by `(batch, half)`.
pub struct SplitPiece(pub AirbenderVerifierInput);

impl StoredObject for SplitPiece {
    const BUCKET: Bucket = Bucket::WitnessInput;
    type Key<'a> = (L1BatchNumber, Half);

    fn encode_key(key: Self::Key<'_>) -> String {
        let (batch, half) = key;
        format!("airbender_split_input_{}_{}.cbor", batch.0, half.suffix())
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        let mut buf = Vec::new();
        ciborium::into_writer(&self.0, &mut buf)
            .map_err(|e| BoxedError::from(format!("Failed to serialize SplitPiece: {e}")))?;
        Ok(buf)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        ciborium::from_reader(&bytes[..])
            .map(Self)
            .map_err(|e| BoxedError::from(format!("Failed to deserialize SplitPiece: {e}")))
    }
}
