use std::fmt;
use std::sync::mpsc::channel;
use std::time::Instant;

use google_cloud_default::WithAuthExt;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::{
    objects::{
        delete::DeleteObjectRequest,
        download::Range,
        get::GetObjectRequest,
        upload::{Media, UploadObjectRequest, UploadType},
    },
    Error::{self, HttpClient},
};
use http::StatusCode;
use tokio;

use zksync_config::ObjectStoreConfig;

use crate::object_store::{ObjectStore, ObjectStoreError};

pub struct GoogleCloudStorage {
    client: Client,
    bucket_prefix: String,
}

// we need to implement custom Debug for GoogleCloudStorage because
// `google_cloud_storage::client::Client` type does not implements debug.
impl fmt::Debug for GoogleCloudStorage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GoogleCloudStorage")
            .field("bucket_prefix", &self.bucket_prefix)
            .finish()
    }
}

impl From<Error> for ObjectStoreError {
    fn from(error: Error) -> Self {
        match error {
            HttpClient(reqwest_error) => {
                if let Some(status) = reqwest_error.status() {
                    match status {
                        StatusCode::NOT_FOUND => {
                            ObjectStoreError::KeyNotFound(reqwest_error.to_string())
                        }
                        _ => ObjectStoreError::Other(reqwest_error.to_string()),
                    }
                } else {
                    ObjectStoreError::Other(reqwest_error.to_string())
                }
            }
            _ => ObjectStoreError::Other(error.to_string()),
        }
    }
}

pub const GOOGLE_CLOUD_STORAGE_OBJECT_STORE_TYPE: &str = "GoogleCloudStorage";

impl GoogleCloudStorage {
    pub fn new(client: Client) -> Self {
        let object_store_config = ObjectStoreConfig::from_env();
        GoogleCloudStorage {
            client,
            bucket_prefix: object_store_config.bucket_base_url,
        }
    }

    fn filename(&self, bucket: &str, filename: &str) -> String {
        format!("{}/{}", bucket, filename)
    }

    async fn get_async(
        self,
        bucket: &'static str,
        key: String,
    ) -> Result<Vec<u8>, ObjectStoreError> {
        let started_at = Instant::now();
        vlog::info!(
            "Fetching data from GCS for key {} from bucket {}",
            &self.filename(bucket, &key),
            self.bucket_prefix
        );
        let blob = self
            .client
            .download_object(
                &GetObjectRequest {
                    bucket: self.bucket_prefix.clone(),
                    object: self.filename(bucket, &key),
                    ..Default::default()
                },
                &Range::default(),
                None,
            )
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
        blob.map_err(ObjectStoreError::from)
    }

    async fn put_async(
        self,
        bucket: &'static str,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), ObjectStoreError> {
        let started_at = Instant::now();
        vlog::info!(
            "Storing data to GCS for key {} from bucket {}",
            &self.filename(bucket, &key),
            self.bucket_prefix
        );
        let upload_type = UploadType::Simple(Media::new(self.filename(bucket, &key)));
        let object = self
            .client
            .upload_object(
                &UploadObjectRequest {
                    bucket: self.bucket_prefix.clone(),
                    ..Default::default()
                },
                value,
                &upload_type,
                None,
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
        object.map(drop).map_err(ObjectStoreError::from)
    }

    async fn remove_async(self, bucket: &'static str, key: String) -> Result<(), ObjectStoreError> {
        vlog::info!(
            "Removing data from GCS for key {} from bucket {}",
            &self.filename(bucket, &key),
            self.bucket_prefix
        );
        self.client
            .delete_object(
                &DeleteObjectRequest {
                    bucket: self.bucket_prefix.clone(),
                    object: self.filename(bucket, &key),
                    ..Default::default()
                },
                None,
            )
            .await
            .map_err(ObjectStoreError::from)
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
        let result = runtime.block_on(async move {
            let gcs_config = ClientConfig::default().with_auth().await.unwrap();
            let gcs = GoogleCloudStorage::new(Client::new(gcs_config));
            query(gcs).await
        });
        tx.send(result).unwrap();
    });
    rx.recv().unwrap()
}

impl ObjectStore for GoogleCloudStorage {
    type Bucket = &'static str;
    type Key = String;
    type Value = Vec<u8>;

    fn get_store_type(&self) -> &'static str {
        GOOGLE_CLOUD_STORAGE_OBJECT_STORE_TYPE
    }

    fn get(&self, bucket: Self::Bucket, key: Self::Key) -> Result<Self::Value, ObjectStoreError> {
        gcs_query(move |gcs| gcs.get_async(bucket, key))
    }

    fn put(
        &mut self,
        bucket: Self::Bucket,
        key: Self::Key,
        value: Self::Value,
    ) -> Result<(), ObjectStoreError> {
        gcs_query(move |gcs| gcs.put_async(bucket, key, value))
    }

    fn remove(&mut self, bucket: Self::Bucket, key: Self::Key) -> Result<(), ObjectStoreError> {
        gcs_query(move |gcs| gcs.remove_async(bucket, key))
    }
}
