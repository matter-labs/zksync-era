use vise::{Counter, EncodeLabelValue, LabeledFamily, Metrics};

/// Why the read-proofs producer failed. A batch-specific failure backs that batch off; the rest
/// hit every batch alike and are retried after the poll interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub(crate) enum ReadProofsErrorKind {
    /// The tree API rules the batch out, e.g. its previous version is pruned.
    TreeApi,
    /// A tree API request failed, e.g. a 5xx or an undecodable response.
    TreeApiRequest,
    /// A proof does not fold to the previous batch root.
    Verification,
    /// A proof disagrees with the value or initialness the witness recorded.
    WitnessMismatch,
    /// The previous batch has no state root in Postgres.
    MissingStateRoot,
    /// A batch blob is missing or cannot be decoded.
    BatchBlob,
    /// Any other object store error, e.g. a network or auth failure.
    ObjectStore,
    Dal,
}

impl ReadProofsErrorKind {
    pub(crate) fn is_batch_specific(self) -> bool {
        !matches!(self, Self::TreeApiRequest | Self::ObjectStore | Self::Dal)
    }
}

#[derive(Debug, Metrics)]
pub(crate) struct AirbenderReadProofsProducerMetrics {
    /// Failed attempts of the read-proofs producer, split by kind. A sustained nonzero rate means
    /// Airbender proving is stalled.
    #[metrics(labels = ["kind"])]
    pub airbender_read_proofs_errors: LabeledFamily<ReadProofsErrorKind, Counter>,
    /// Read proofs stored by the producer.
    pub airbender_read_proofs_stored: Counter,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<AirbenderReadProofsProducerMetrics> = vise::Global::new();
