use std::{str::FromStr, sync::Arc};

use anyhow::anyhow;
use async_trait::async_trait;
use secp256k1::SecretKey;
use subxt_signer::ExposeSecret;
use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};

use super::{blob_info::BlobInfo, sdk::RawEigenClient};
use crate::utils::to_non_retriable_da_error;

/// EigenClient is a client for the Eigen DA service.
/// It can be configured to use one of two dispersal methods:
/// - Remote: Dispatch blobs to a remote Eigen service.
/// - Memstore: Stores blobs in memory, used for testing purposes.
#[derive(Debug, Clone)]
pub struct EigenClient {
    client: Arc<RawEigenClient>,
}

impl EigenClient {
    pub async fn new(config: EigenConfig, secrets: EigenSecrets) -> anyhow::Result<Self> {
        let private_key = SecretKey::from_str(secrets.private_key.0.expose_secret().as_str())
            .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;

        let client = RawEigenClient::new(private_key, config).await?;
        Ok(Self {
            client: Arc::new(client),
        })
    }

    pub async fn get_commitment(&self, blob_id: &str) -> anyhow::Result<String> {
        let blob_info = self.client.get_inclusion_data(blob_id).await?;
        Ok(blob_info)
    }
}

#[async_trait]
impl DataAvailabilityClient for EigenClient {
    async fn dispatch_blob(
        &self,
        _: u32, // batch number
        data: Vec<u8>,
    ) -> Result<DispatchResponse, DAError> {
        let blob_id = self
            .client
            .dispatch_blob(data)
            .await
            .map_err(to_non_retriable_da_error)?;

        Ok(DispatchResponse::from(blob_id))
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        let blob_info = self
            .get_commitment(blob_id)
            .await
            .map_err(to_non_retriable_da_error)?;
        let rlp_encoded_bytes = hex::decode(blob_info).map_err(|_| DAError {
            error: anyhow!("Failed to decode blob_id"),
            is_retriable: false,
        })?;
        let blob_info: BlobInfo = rlp::decode(&rlp_encoded_bytes).map_err(|_| DAError {
            error: anyhow!("Failed to decode blob_info"),
            is_retriable: false,
        })?;
        let inclusion_data = blob_info.blob_verification_proof.inclusion_proof;
        Ok(Some(InclusionData {
            data: inclusion_data,
        }))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(RawEigenClient::blob_size_limit())
    }
}

#[cfg(test)]
impl EigenClient {
    pub async fn get_blob_data(&self, blob_id: &str) -> anyhow::Result<Option<Vec<u8>>, DAError> {
        self.client.get_blob_data(blob_id).await
    }
}
#[cfg(test)]
mod tests {
    use serial_test::serial;
    use zksync_types::secrets::PrivateKey;

    use super::*;
    use crate::eigen::blob_info::BlobInfo;

    #[tokio::test]
    #[serial]
    async fn test_non_auth_dispersal() {
        let config = EigenConfig {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            settlement_layer_confirmation_depth: -1,
            eigenda_eth_rpc: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            eigenda_svc_manager_address: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            status_query_timeout: 1800000, // 30 minutes
            status_query_interval: 5,      // 5 ms
            wait_for_finalization: false,
            authenticated: false,
            verify_cert: true,
            path_to_points: "../../../resources".to_string(),
            chain_id: 17000,
        };
        let secrets = EigenSecrets {
            private_key: PrivateKey::from_str(
                "d08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6",
            )
            .unwrap(),
        };
        let client = EigenClient::new(config, secrets).await.unwrap();
        let data = vec![1; 20];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();

        let blob_info_str = client.get_commitment(&result.blob_id).await.unwrap();

        let blob_info: BlobInfo =
            rlp::decode(&hex::decode(blob_info_str.clone()).unwrap()).unwrap();
        let expected_inclusion_data = blob_info.blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);
        let retrieved_data = client.get_blob_data(&blob_info_str).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }

    #[tokio::test]
    #[serial]
    async fn test_auth_dispersal() {
        let config = EigenConfig {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            settlement_layer_confirmation_depth: -1,
            eigenda_eth_rpc: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            eigenda_svc_manager_address: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            status_query_timeout: 1800000, // 30 minutes
            status_query_interval: 5,      // 5 ms
            wait_for_finalization: false,
            authenticated: true,
            verify_cert: true,
            path_to_points: "../../../resources".to_string(),
            chain_id: 17000,
        };
        let secrets = EigenSecrets {
            private_key: PrivateKey::from_str(
                "d08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6",
            )
            .unwrap(),
        };
        let client = EigenClient::new(config, secrets).await.unwrap();
        let data = vec![1; 20];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();
        let blob_info_str = client.get_commitment(&result.blob_id).await.unwrap();

        let blob_info: BlobInfo =
            rlp::decode(&hex::decode(blob_info_str.clone()).unwrap()).unwrap();
        let expected_inclusion_data = blob_info.blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);
        let retrieved_data = client.get_blob_data(&blob_info_str).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }

    #[tokio::test]
    #[serial]
    async fn test_wait_for_finalization() {
        let config = EigenConfig {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            status_query_timeout: 1800000, // 30 minutes
            status_query_interval: 5000,   // 5000 ms
            wait_for_finalization: true,
            authenticated: true,
            verify_cert: true,
            path_to_points: "../../../resources".to_string(),
            settlement_layer_confirmation_depth: 0,
            eigenda_eth_rpc: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            eigenda_svc_manager_address: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            chain_id: 17000,
        };
        let secrets = EigenSecrets {
            private_key: PrivateKey::from_str(
                "d08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6",
            )
            .unwrap(),
        };
        let client = EigenClient::new(config, secrets).await.unwrap();
        let data = vec![1; 20];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();
        let blob_info_str = client.get_commitment(&result.blob_id).await.unwrap();

        let blob_info: BlobInfo =
            rlp::decode(&hex::decode(blob_info_str.clone()).unwrap()).unwrap();
        let expected_inclusion_data = blob_info.blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);
        let retrieved_data = client.get_blob_data(&blob_info_str).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }

    #[tokio::test]
    #[serial]
    async fn test_settlement_layer_confirmation_depth() {
        let config = EigenConfig {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            settlement_layer_confirmation_depth: 5,
            eigenda_eth_rpc: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            eigenda_svc_manager_address: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            status_query_timeout: 1800000, // 30 minutes
            status_query_interval: 5,      // 5 ms
            wait_for_finalization: false,
            authenticated: false,
            verify_cert: true,
            path_to_points: "../../../resources".to_string(),
            chain_id: 17000,
        };
        let secrets = EigenSecrets {
            private_key: PrivateKey::from_str(
                "d08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6",
            )
            .unwrap(),
        };
        let client = EigenClient::new(config, secrets).await.unwrap();
        let data = vec![1; 20];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();
        let blob_info_str = client.get_commitment(&result.blob_id).await.unwrap();

        let blob_info: BlobInfo =
            rlp::decode(&hex::decode(blob_info_str.clone()).unwrap()).unwrap();
        let expected_inclusion_data = blob_info.blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);
        let retrieved_data = client.get_blob_data(&blob_info_str).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }

    #[tokio::test]
    #[serial]
    async fn test_auth_dispersal_settlement_layer_confirmation_depth() {
        let config = EigenConfig {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            settlement_layer_confirmation_depth: 5,
            eigenda_eth_rpc: "https://ethereum-holesky-rpc.publicnode.com".to_string(),
            eigenda_svc_manager_address: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            status_query_timeout: 1800000, // 30 minutes
            status_query_interval: 5,      // 5 ms
            wait_for_finalization: false,
            authenticated: true,
            verify_cert: true,
            path_to_points: "../../../resources".to_string(),
            chain_id: 17000,
        };
        let secrets = EigenSecrets {
            private_key: PrivateKey::from_str(
                "d08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6",
            )
            .unwrap(),
        };
        let client = EigenClient::new(config, secrets).await.unwrap();
        let data = vec![1; 20];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();
        let blob_info_str = client.get_commitment(&result.blob_id).await.unwrap();

        let blob_info: BlobInfo =
            rlp::decode(&hex::decode(blob_info_str.clone()).unwrap()).unwrap();
        let expected_inclusion_data = blob_info.blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);
        let retrieved_data = client.get_blob_data(&blob_info_str).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }
}
