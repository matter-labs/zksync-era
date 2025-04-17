use vise::{Histogram, Metrics};
use zksync_object_store::bincode;
use zksync_prover_interface::inputs::WitnessInputData;

const BYTES_IN_MEGABYTE: u64 = 1024 * 1024;

#[derive(Debug, Metrics)]
pub(super) struct ProofDataHandlerMetrics {
    #[metrics(buckets = vise::Buckets::exponential(1.0..=2_048.0, 2.0))]
    pub vm_run_data_blob_size_in_mb: Histogram<u64>,
    #[metrics(buckets = vise::Buckets::exponential(1.0..=2_048.0, 2.0))]
    pub merkle_paths_blob_size_in_mb: Histogram<u64>,
    #[metrics(buckets = vise::Buckets::exponential(1.0..=2_048.0, 2.0))]
    pub eip_4844_blob_size_in_mb: Histogram<u64>,
    #[metrics(buckets = vise::Buckets::exponential(1.0..=2_048.0, 2.0))]
    pub total_blob_size_in_mb: Histogram<u64>,
}

impl ProofDataHandlerMetrics {
    pub fn observe_blob_sizes(&self, blob: &WitnessInputData) {
        let vm_run_data_blob_size_in_mb =
            bincode::serialize(&blob.vm_run_data).unwrap().len() as u64 / BYTES_IN_MEGABYTE;
        let merkle_paths_blob_size_in_mb =
            bincode::serialize(&blob.merkle_paths).unwrap().len() as u64 / BYTES_IN_MEGABYTE;
        let eip_4844_blob_size_in_mb =
            bincode::serialize(&blob.eip_4844_blobs).unwrap().len() as u64 / BYTES_IN_MEGABYTE;
        let total_blob_size_in_mb =
            bincode::serialize(blob).unwrap().len() as u64 / BYTES_IN_MEGABYTE;

        self.vm_run_data_blob_size_in_mb
            .observe(vm_run_data_blob_size_in_mb);
        self.merkle_paths_blob_size_in_mb
            .observe(merkle_paths_blob_size_in_mb);
        self.eip_4844_blob_size_in_mb
            .observe(eip_4844_blob_size_in_mb);
        self.total_blob_size_in_mb.observe(total_blob_size_in_mb);
    }
}

#[vise::register]
pub(super) static METRICS: vise::Global<ProofDataHandlerMetrics> = vise::Global::new();
