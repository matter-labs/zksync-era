use zksync_dal::ConnectionPool;
use zksync_object_store::{Bucket, ObjectStore, ObjectStoreError, ObjectStoreFactory};

use crate::house_keeper::periodic_job::PeriodicJob;

trait AsBlobUrls {
    fn as_blob_urls(&self) -> (&str, Option<&str>);
}

impl AsBlobUrls for String {
    fn as_blob_urls(&self) -> (&str, Option<&str>) {
        (self.as_str(), None)
    }
}

impl AsBlobUrls for (String, String) {
    fn as_blob_urls(&self) -> (&str, Option<&str>) {
        (self.0.as_str(), Some(self.1.as_str()))
    }
}

#[derive(Debug)]
pub struct GcsBlobCleaner {
    object_store: Box<dyn ObjectStore>,
    cleaning_interval_ms: u64,
}

const BATCH_CLEANUP_SIZE: u8 = 5;

fn handle_remove_result(result: Result<(), ObjectStoreError>) {
    if let Err(error) = result {
        match error {
            // There can be scenario when the removal from the GCS succeeded and updating the DB after that fails,
            // in this scenario the retry of removal from GCS would fail as the object is already removed.
            // Hence we ignore the KeyNotFound error below
            ObjectStoreError::KeyNotFound(_) => {}
            other => panic!("{:?}", other),
        }
    }
}

impl GcsBlobCleaner {
    pub fn new(store_factory: &ObjectStoreFactory, cleaning_interval_ms: u64) -> Self {
        Self {
            object_store: store_factory.create_store(),
            cleaning_interval_ms,
        }
    }

    fn cleanup_blobs(&mut self, pool: ConnectionPool) {
        self.cleanup_prover_jobs_blobs(pool.clone());
        self.cleanup_witness_inputs_blobs(pool.clone());
        self.cleanup_leaf_aggregation_witness_jobs_blobs(pool.clone());
        self.cleanup_node_aggregation_witness_jobs_blobs(pool.clone());
        self.cleanup_scheduler_witness_jobs_blobs(pool);
    }

    fn cleanup_prover_jobs_blobs(&self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();
        let blob_urls = conn
            .prover_dal()
            .get_circuit_input_blob_urls_to_be_cleaned(BATCH_CLEANUP_SIZE);
        let ids = self.cleanup_blob_urls(Bucket::ProverJobs, blob_urls);
        conn.prover_dal().mark_gcs_blobs_as_cleaned(ids);
    }

    fn cleanup_blob_urls<S: AsBlobUrls>(
        &self,
        bucket: Bucket,
        blob_urls: Vec<(i64, S)>,
    ) -> Vec<i64> {
        if !blob_urls.is_empty() {
            vlog::info!("Found {} {} for cleaning blobs", blob_urls.len(), bucket);
        }

        for (_, url) in &blob_urls {
            let (first_url, second_url) = url.as_blob_urls();
            handle_remove_result(self.object_store.remove_raw(bucket, first_url));
            if let Some(second_url) = second_url {
                handle_remove_result(self.object_store.remove_raw(bucket, second_url));
            }
        }
        blob_urls.into_iter().map(|(id, _)| id).collect()
    }

    fn cleanup_witness_inputs_blobs(&self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();
        let blob_urls = conn
            .blocks_dal()
            .get_merkle_tree_paths_blob_urls_to_be_cleaned(BATCH_CLEANUP_SIZE);
        let l1_batch_numbers = self.cleanup_blob_urls(Bucket::WitnessInput, blob_urls);
        conn.blocks_dal()
            .mark_gcs_blobs_as_cleaned(l1_batch_numbers);
    }

    fn cleanup_leaf_aggregation_witness_jobs_blobs(&mut self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();

        let blob_urls = conn
            .witness_generator_dal()
            .get_basic_circuit_and_circuit_inputs_blob_urls_to_be_cleaned(BATCH_CLEANUP_SIZE);
        let l1_batch_numbers =
            self.cleanup_blob_urls(Bucket::LeafAggregationWitnessJobs, blob_urls);
        conn.witness_generator_dal()
            .mark_leaf_aggregation_gcs_blobs_as_cleaned(l1_batch_numbers);
    }

    fn cleanup_node_aggregation_witness_jobs_blobs(&mut self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();
        let blob_urls = conn
            .witness_generator_dal()
            .get_leaf_layer_subqueues_and_aggregation_outputs_blob_urls_to_be_cleaned(
                BATCH_CLEANUP_SIZE,
            );
        let l1_batch_numbers =
            self.cleanup_blob_urls(Bucket::NodeAggregationWitnessJobs, blob_urls);
        conn.witness_generator_dal()
            .mark_node_aggregation_gcs_blobs_as_cleaned(l1_batch_numbers);
    }

    fn cleanup_scheduler_witness_jobs_blobs(&mut self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();
        let blob_urls = conn
            .witness_generator_dal()
            .get_scheduler_witness_and_node_aggregations_blob_urls_to_be_cleaned(
                BATCH_CLEANUP_SIZE,
            );
        let l1_batch_numbers = self.cleanup_blob_urls(Bucket::SchedulerWitnessJobs, blob_urls);
        conn.witness_generator_dal()
            .mark_scheduler_witness_gcs_blobs_as_cleaned(l1_batch_numbers);
    }
}

impl PeriodicJob for GcsBlobCleaner {
    const SERVICE_NAME: &'static str = "GcsBlobCleaner";

    fn run_routine_task(&mut self, connection_pool: ConnectionPool) {
        self.cleanup_blobs(connection_pool);
    }

    fn polling_interval_ms(&self) -> u64 {
        self.cleaning_interval_ms
    }
}
