pub use cloud_storage;
use cloud_storage::Client;
use std::env;
use std::error::Error;
use std::sync::mpsc::channel;
use std::time::Instant;
use tokio;

use zksync_config::ObjectStoreConfig;

use crate::object_store::ObjectStore;

#[derive(Debug)]
pub struct GoogleCloudStorage {
    client: Client,
    bucket_prefix: String,
}
pub const GOOGLE_CLOUD_STORAGE_OBJECT_STORE_TYPE: &str = "GoogleCloudStorage";

impl GoogleCloudStorage {
    pub fn new() -> Self {
        let config = ObjectStoreConfig::from_env();
        env::set_var("SERVICE_ACCOUNT", config.service_account_path);
        GoogleCloudStorage {
            client: Client::new(),
            bucket_prefix: ObjectStoreConfig::from_env().bucket_base_url,
        }
    }

    fn filename(&self, bucket: &str, filename: &str) -> String {
        format!("{}/{}", bucket, filename)
    }

    async fn get_async(
        self,
        bucket: &'static str,
        key: String,
    ) -> Result<Vec<u8>, cloud_storage::Error> {
        let started_at = Instant::now();
        vlog::info!(
            "Fetching data from GCS for key {} from bucket {}",
            &self.filename(bucket, &key),
            self.bucket_prefix
        );
        let blob = self
            .client
            .object()
            .download(&self.bucket_prefix, &self.filename(bucket, &key))
            .await;
        vlog::info!(
            "Fetched data from GCS for key {} from bucket {} and it took: {:?}",
            key,
            bucket,
            started_at.elapsed()
        );
        metrics::histogram!(
            "server.object_store.fetching_time",
            started_at.elapsed(),
            "bucket" => bucket
        );
        blob
    }

    async fn put_async(
        self,
        bucket: &'static str,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), cloud_storage::Error> {
        let started_at = Instant::now();
        vlog::info!(
            "Storing data to GCS for key {} from bucket {}",
            &self.filename(bucket, &key),
            self.bucket_prefix
        );
        let object = self
            .client
            .object()
            .create(
                &self.bucket_prefix,
                value,
                &self.filename(bucket, &key),
                "binary/blob",
            )
            .await;
        vlog::info!(
            "Stored data to GCS for key {} from bucket {} and it took: {:?}",
            key,
            bucket,
            started_at.elapsed()
        );
        metrics::histogram!(
            "server.object_store.storing_time",
            started_at.elapsed(),
            "bucket" => bucket
        );
        object.map(drop)
    }

    async fn remove_async(
        self,
        bucket: &'static str,
        key: String,
    ) -> Result<(), cloud_storage::Error> {
        vlog::info!(
            "Removing data from GCS for key {} from bucket {}",
            &self.filename(bucket, &key),
            self.bucket_prefix
        );
        self.client
            .object()
            .delete(&self.bucket_prefix, &self.filename(bucket, &key))
            .await
    }
}

impl Default for GoogleCloudStorage {
    fn default() -> Self {
        Self::new()
    }
}

fn gcs_query<F, FUT, OUT>(query: F) -> OUT
where
    OUT: Send + 'static,
    FUT: std::future::Future<Output = OUT>,
    F: FnOnce(GoogleCloudStorage) -> FUT + Send + 'static,
{
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap();
        let gcs = GoogleCloudStorage::new();
        let result = runtime.block_on(Box::pin(query(gcs)));
        tx.send(result).unwrap();
    });
    rx.recv().unwrap()
}

impl ObjectStore for GoogleCloudStorage {
    type Bucket = &'static str;
    type Key = String;
    type Value = Vec<u8>;
    type Error = Box<dyn Error>;

    fn get_store_type(&self) -> &'static str {
        GOOGLE_CLOUD_STORAGE_OBJECT_STORE_TYPE
    }

    fn get(&self, bucket: Self::Bucket, key: Self::Key) -> Result<Self::Value, Self::Error> {
        gcs_query(move |gcs| gcs.get_async(bucket, key)).map_err(|e| e.into())
    }

    fn put(
        &mut self,
        bucket: Self::Bucket,
        key: Self::Key,
        value: Self::Value,
    ) -> Result<(), Self::Error> {
        gcs_query(move |gcs| gcs.put_async(bucket, key, value)).map_err(|e| e.into())
    }

    fn remove(&mut self, bucket: Self::Bucket, key: Self::Key) -> Result<(), Self::Error> {
        gcs_query(move |gcs| gcs.remove_async(bucket, key)).map_err(|e| e.into())
    }
}
