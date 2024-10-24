use std::sync::Arc;

use axum::{extract::Path, http::Response};

use crate::{eigenda_client::EigenDAClient, errors::RequestProcessorError, memstore::MemStore};

#[derive(Clone)]
pub(crate) enum ClientType {
    Memory(Arc<MemStore>),
    Disperser(EigenDAClient),
}

impl ClientType {
    async fn get_blob(&self, blob_id: Vec<u8>) -> Result<Vec<u8>, RequestProcessorError> {
        match self {
            Self::Memory(memstore) => memstore
                .clone()
                .get_blob(blob_id)
                .await
                .map_err(|e| RequestProcessorError::MemStore(e)),
            Self::Disperser(disperser) => disperser
                .get_blob(blob_id)
                .await
                .map_err(|e| RequestProcessorError::EigenDA(e)),
        }
    }

    async fn put_blob(&self, data: Vec<u8>) -> Result<Vec<u8>, RequestProcessorError> {
        match self {
            Self::Memory(memstore) => memstore
                .clone()
                .put_blob(data)
                .await
                .map_err(|e| RequestProcessorError::MemStore(e)),
            Self::Disperser(disperser) => disperser
                .put_blob(data)
                .await
                .map_err(|e| RequestProcessorError::EigenDA(e)),
        }
    }
}

#[derive(Clone)]
pub(crate) struct RequestProcessorNew {
    client: ClientType,
}

impl RequestProcessorNew {
    pub(crate) fn new(client: ClientType) -> Self {
        Self { client }
    }

    pub(crate) async fn get_blob_id(
        &self,
        Path(blob_id): Path<String>,
    ) -> Result<axum::response::Response, RequestProcessorError> {
        let blob_id_bytes = hex::decode(blob_id).unwrap();
        let response = self.client.get_blob(blob_id_bytes).await.unwrap();
        Ok(Response::new(response.into()))
    }

    pub(crate) async fn put_blob_id(
        &self,
        Path(data): Path<String>,
    ) -> Result<axum::response::Response, RequestProcessorError> {
        let data_bytes = hex::decode(data).unwrap();
        let response = self.client.put_blob(data_bytes).await.unwrap();
        Ok(Response::new(response.into()))
    }
}
