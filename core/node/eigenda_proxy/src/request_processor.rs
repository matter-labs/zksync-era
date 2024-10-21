use std::sync::Arc;

use axum::{
    extract::Path,
    http::{header, Response, StatusCode},
    Json,
};
use tokio::sync::Mutex;
use tonic::transport::Channel;

use crate::{
    disperser::{disperser_client::DisperserClient, BlobStatus, BlobStatusRequest},
    eigenda_client::EigenDAClient,
    errors::RequestProcessorError,
};

#[derive(Clone)]
pub(crate) struct RequestProcessor {
    eigenda_client: EigenDAClient,
}

impl RequestProcessor {
    pub(crate) fn new(eigenda_client: EigenDAClient) -> Self {
        Self { eigenda_client }
    }

    pub(crate) async fn get_blob_id(
        &self,
        Path(blob_id): Path<String>,
    ) -> axum::response::Response {
        let blob_id_bytes = hex::decode(blob_id).unwrap();
        let response = self.eigenda_client.get_blob(blob_id_bytes).await.unwrap();
        Response::new(response.into())
    }

    pub(crate) async fn put_blob_id(&self, Path(data): Path<String>) -> axum::response::Response {
        let data_bytes = hex::decode(data).unwrap();
        let response = self.eigenda_client.put_blob(data_bytes).await.unwrap();
        Response::new(response.into())
    }
}
