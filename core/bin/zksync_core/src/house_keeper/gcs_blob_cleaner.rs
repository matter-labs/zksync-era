use std::error;

use zksync_dal::ConnectionPool;
use zksync_object_store::cloud_storage::Reason;
use zksync_object_store::gcs_object_store::cloud_storage::Error;
use zksync_object_store::gcs_object_store::GOOGLE_CLOUD_STORAGE_OBJECT_STORE_TYPE;
use zksync_object_store::object_store::{
    DynamicObjectStore, LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH,
    NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH, PROVER_JOBS_BUCKET_PATH,
    SCHEDULER_WITNESS_JOBS_BUCKET_PATH, WITNESS_INPUT_BUCKET_PATH,
};

use crate::house_keeper::periodic_job::PeriodicJob;

#[derive(Debug)]
pub struct GcsBlobCleaner {
    pub object_store: DynamicObjectStore,
}

const BATCH_CLEANUP_SIZE: u8 = 5;

fn handle_remove_result(object_store_type: &str, result: Result<(), Box<dyn error::Error>>) {
    if object_store_type == GOOGLE_CLOUD_STORAGE_OBJECT_STORE_TYPE {
        match result {
            Ok(_) => {} // DO NOTHING
            Err(err) => {
                let gcs_error = err.downcast::<Error>().unwrap();
                match *gcs_error {
                    Error::Google(err) => {
                        if err
                            .error
                            .errors
                            .iter()
                            .any(|err| matches!(err.reason, Reason::NotFound))
                        {
                            return;
                        }
                        panic!("{:?}", err)
                    }
                    _ => {
                        panic!("{:?}", gcs_error)
                    }
                }
            }
        }
    }
}

/// There can be scenario when the removal from the GCS succeeded and updating the DB after that fails,
/// in this scenario the retry of removal from GCS would fail as the object is already removed.
/// To handle this either the `Key does not exist` error from GCS can be ignored or other option is to do everything inside a transaction.
impl GcsBlobCleaner {
    fn cleanup_blobs(&mut self, pool: ConnectionPool) {
        self.cleanup_prover_jobs_blobs(pool.clone());
        self.cleanup_witness_inputs_blobs(pool.clone());
        self.cleanup_leaf_aggregation_witness_jobs_blobs(pool.clone());
        self.cleanup_node_aggregation_witness_jobs_blobs(pool.clone());
        self.cleanup_scheduler_witness_jobs_blobs(pool);
    }

    fn cleanup_prover_jobs_blobs(&mut self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();
        let id_blob_urls_tuple = conn
            .prover_dal()
            .get_circuit_input_blob_urls_to_be_cleaned(BATCH_CLEANUP_SIZE);
        let (ids, circuit_input_blob_urls): (Vec<_>, Vec<_>) =
            id_blob_urls_tuple.into_iter().unzip();

        vlog::info!("Found {} provers jobs for cleaning blobs", ids.len());

        circuit_input_blob_urls.into_iter().for_each(|url| {
            handle_remove_result(
                self.object_store.get_store_type(),
                self.object_store.remove(PROVER_JOBS_BUCKET_PATH, url),
            );
        });

        conn.prover_dal().mark_gcs_blobs_as_cleaned(ids);
    }

    fn cleanup_witness_inputs_blobs(&mut self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();
        let l1_batches_blob_urls_tuple = conn
            .blocks_dal()
            .get_merkle_tree_paths_blob_urls_to_be_cleaned(BATCH_CLEANUP_SIZE);
        let (l1_batch_numbers, merkle_tree_paths_blob_urls): (Vec<_>, Vec<_>) =
            l1_batches_blob_urls_tuple.into_iter().unzip();

        vlog::info!(
            "Found {} witness inputs for cleaning blobs",
            l1_batch_numbers.len()
        );

        merkle_tree_paths_blob_urls.into_iter().for_each(|url| {
            handle_remove_result(
                self.object_store.get_store_type(),
                self.object_store.remove(WITNESS_INPUT_BUCKET_PATH, url),
            );
        });
        conn.blocks_dal()
            .mark_gcs_blobs_as_cleaned(l1_batch_numbers);
    }

    fn cleanup_leaf_aggregation_witness_jobs_blobs(&mut self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();

        let l1_batches_blob_urls_tuple = conn
            .witness_generator_dal()
            .get_basic_circuit_and_circuit_inputs_blob_urls_to_be_cleaned(BATCH_CLEANUP_SIZE);
        let (l1_batch_numbers, basic_circuit_and_circuit_inputs_blob_urls): (Vec<_>, Vec<_>) =
            l1_batches_blob_urls_tuple.into_iter().unzip();

        vlog::info!(
            "Found {} leaf aggregation witness jobs for cleaning blobs",
            l1_batch_numbers.len()
        );

        basic_circuit_and_circuit_inputs_blob_urls
            .into_iter()
            .for_each(|url_pair| {
                handle_remove_result(
                    self.object_store.get_store_type(),
                    self.object_store
                        .remove(LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH, url_pair.0),
                );
                handle_remove_result(
                    self.object_store.get_store_type(),
                    self.object_store
                        .remove(LEAF_AGGREGATION_WITNESS_JOBS_BUCKET_PATH, url_pair.1),
                );
            });

        conn.witness_generator_dal()
            .mark_leaf_aggregation_gcs_blobs_as_cleaned(l1_batch_numbers);
    }

    fn cleanup_node_aggregation_witness_jobs_blobs(&mut self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();
        let l1_batches_blob_urls_tuple = conn
            .witness_generator_dal()
            .get_leaf_layer_subqueues_and_aggregation_outputs_blob_urls_to_be_cleaned(
                BATCH_CLEANUP_SIZE,
            );

        let (l1_batch_numbers, leaf_layer_subqueues_and_aggregation_outputs_blob_urls): (
            Vec<_>,
            Vec<_>,
        ) = l1_batches_blob_urls_tuple.into_iter().unzip();

        vlog::info!(
            "Found {} node aggregation witness jobs for cleaning blobs",
            l1_batch_numbers.len()
        );

        leaf_layer_subqueues_and_aggregation_outputs_blob_urls
            .into_iter()
            .for_each(|url_pair| {
                handle_remove_result(
                    self.object_store.get_store_type(),
                    self.object_store
                        .remove(NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH, url_pair.0),
                );
                handle_remove_result(
                    self.object_store.get_store_type(),
                    self.object_store
                        .remove(NODE_AGGREGATION_WITNESS_JOBS_BUCKET_PATH, url_pair.1),
                );
            });
        conn.witness_generator_dal()
            .mark_node_aggregation_gcs_blobs_as_cleaned(l1_batch_numbers);
    }

    fn cleanup_scheduler_witness_jobs_blobs(&mut self, pool: ConnectionPool) {
        let mut conn = pool.access_storage_blocking();
        let l1_batches_blob_urls_tuple = conn
            .witness_generator_dal()
            .get_scheduler_witness_and_node_aggregations_blob_urls_to_be_cleaned(
                BATCH_CLEANUP_SIZE,
            );

        let (l1_batch_numbers, scheduler_witness_and_node_aggregations_blob_urls): (
            Vec<_>,
            Vec<_>,
        ) = l1_batches_blob_urls_tuple.into_iter().unzip();

        vlog::info!(
            "Found {} scheduler witness jobs for cleaning blobs",
            l1_batch_numbers.len()
        );

        scheduler_witness_and_node_aggregations_blob_urls
            .into_iter()
            .for_each(|url_pair| {
                handle_remove_result(
                    self.object_store.get_store_type(),
                    self.object_store
                        .remove(SCHEDULER_WITNESS_JOBS_BUCKET_PATH, url_pair.0),
                );
                handle_remove_result(
                    self.object_store.get_store_type(),
                    self.object_store
                        .remove(SCHEDULER_WITNESS_JOBS_BUCKET_PATH, url_pair.1),
                );
            });
        conn.witness_generator_dal()
            .mark_scheduler_witness_gcs_blobs_as_cleaned(l1_batch_numbers);
    }
}

impl PeriodicJob for GcsBlobCleaner {
    const SERVICE_NAME: &'static str = "GcsBlobCleaner";
    const POLLING_INTERVAL_MS: u64 = 5000;

    fn run_routine_task(&mut self, connection_pool: ConnectionPool) {
        self.cleanup_blobs(connection_pool);
    }
}
