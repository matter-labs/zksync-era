use std::{str::FromStr, sync::Arc};

use async_trait::async_trait;
use secp256k1::SecretKey;
use subxt_signer::ExposeSecret;
use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};

use super::sdk::RawEigenClient;
use crate::utils::to_retriable_da_error;

#[async_trait]
pub trait GetBlobData: std::fmt::Debug + Send + Sync {
    async fn get_blob_data(&self, input: &str) -> anyhow::Result<Option<Vec<u8>>>;
}

/// EigenClient is a client for the Eigen DA service.
#[derive(Debug, Clone)]
pub struct EigenClient {
    pub(crate) client: Arc<RawEigenClient>,
}

impl EigenClient {
    pub async fn new(
        config: EigenConfig,
        secrets: EigenSecrets,
        get_blob_data: Arc<dyn GetBlobData>,
    ) -> anyhow::Result<Self> {
        let private_key = SecretKey::from_str(secrets.private_key.0.expose_secret().as_str())
            .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;

        let client = RawEigenClient::new(private_key, config, get_blob_data).await?;
        Ok(Self {
            client: Arc::new(client),
        })
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
            .map_err(to_retriable_da_error)?;

        Ok(DispatchResponse::from(blob_id))
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        let inclusion_data = self
            .client
            .get_inclusion_data(blob_id)
            .await
            .map_err(to_retriable_da_error)?;
        if let Some(inclusion_data) = inclusion_data {
            Ok(Some(InclusionData {
                data: inclusion_data,
            }))
        } else {
            Ok(None)
        }
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(RawEigenClient::blob_size_limit())
    }
}

/// EigenDA Client tests are ignored by default, because they require a remote dependency,
/// which may not always be available, causing tests to be flaky.
/// To run these tests, use the following command:
/// `cargo test -p zksync_da_clients -- --ignored`
#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc, time::Duration};

    use backon::{ConstantBuilder, Retryable};
    use serial_test::file_serial;
    use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
    use zksync_da_client::{
        types::{DAError, DispatchResponse},
        DataAvailabilityClient,
    };
    use zksync_types::secrets::PrivateKey;

    use crate::eigen::{blob_info::BlobInfo, EigenClient, GetBlobData};

    impl EigenClient {
        async fn get_blob_data(
            &self,
            blob_id: BlobInfo,
        ) -> anyhow::Result<Option<Vec<u8>>, DAError> {
            self.client.get_blob_data(blob_id).await
        }

        async fn get_commitment(&self, blob_id: &str) -> anyhow::Result<Option<BlobInfo>> {
            self.client.get_commitment(blob_id).await
        }
    }

    const STATUS_QUERY_TIMEOUT: u64 = 1800000; // 30 minutes
    const STATUS_QUERY_INTERVAL: u64 = 5; // 5 ms

    async fn get_blob_info(
        client: &EigenClient,
        result: &DispatchResponse,
    ) -> anyhow::Result<BlobInfo> {
        let blob_info = (|| async {
            let blob_info = client.get_commitment(&result.blob_id).await?;
            if blob_info.is_none() {
                return Err(anyhow::anyhow!("Blob not found"));
            }
            Ok(blob_info.unwrap())
        })
        .retry(
            &ConstantBuilder::default()
                .with_delay(Duration::from_millis(STATUS_QUERY_INTERVAL))
                .with_max_times((STATUS_QUERY_TIMEOUT / STATUS_QUERY_INTERVAL) as usize),
        )
        .when(|e| e.to_string().contains("Blob not found"))
        .await?;

        Ok(blob_info)
    }

    #[derive(Debug, Clone)]
    struct MockGetBlobData;

    #[async_trait::async_trait]
    impl GetBlobData for MockGetBlobData {
        async fn get_blob_data(&self, _input: &'_ str) -> anyhow::Result<Option<Vec<u8>>> {
            Ok(None)
        }
    }

    fn test_secrets() -> EigenSecrets {
        EigenSecrets {
            private_key: PrivateKey::from_str(
                "d08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6",
            )
            .unwrap(),
        }
    }

    #[ignore = "depends on external RPC"]
    #[tokio::test]
    #[file_serial]
    async fn test_non_auth_dispersal() {
        let config = EigenConfig::default();
        let secrets = test_secrets();
        let client = EigenClient::new(config.clone(), secrets, Arc::new(MockGetBlobData))
            .await
            .unwrap();
        let data = vec![1; 20];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();

        let blob_info = get_blob_info(&client, &result).await.unwrap();
        let expected_inclusion_data = blob_info.clone().blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);
        let retrieved_data = client.get_blob_data(blob_info).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }

    #[ignore = "depends on external RPC"]
    #[tokio::test]
    #[file_serial]
    async fn test_auth_dispersal() {
        let config = EigenConfig {
            authenticated: true,
            ..EigenConfig::default()
        };
        let secrets = test_secrets();
        let client = EigenClient::new(config.clone(), secrets, Arc::new(MockGetBlobData))
            .await
            .unwrap();
        let data = vec![1; 20];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();
        let blob_info = get_blob_info(&client, &result).await.unwrap();

        let expected_inclusion_data = blob_info.clone().blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);
        let retrieved_data = client.get_blob_data(blob_info).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }

    #[ignore = "depends on external RPC"]
    #[tokio::test]
    #[file_serial]
    async fn test_wait_for_finalization() {
        let config = EigenConfig {
            wait_for_finalization: true,
            authenticated: true,
            ..EigenConfig::default()
        };
        let secrets = test_secrets();

        let client = EigenClient::new(config.clone(), secrets, Arc::new(MockGetBlobData))
            .await
            .unwrap();
        let data = vec![1; 20];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();
        let blob_info = get_blob_info(&client, &result).await.unwrap();

        let expected_inclusion_data = blob_info.clone().blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);
        let retrieved_data = client.get_blob_data(blob_info).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }

    #[ignore = "depends on external RPC"]
    #[tokio::test]
    #[file_serial]
    async fn test_settlement_layer_confirmation_depth() {
        let config = EigenConfig {
            settlement_layer_confirmation_depth: 5,
            ..EigenConfig::default()
        };
        let secrets = test_secrets();
        let client = EigenClient::new(config.clone(), secrets, Arc::new(MockGetBlobData))
            .await
            .unwrap();
        let data = vec![1; 20];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();
        let blob_info = get_blob_info(&client, &result).await.unwrap();

        let expected_inclusion_data = blob_info.clone().blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);
        let retrieved_data = client.get_blob_data(blob_info).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }

    #[ignore = "depends on external RPC"]
    #[tokio::test]
    #[file_serial]
    async fn test_auth_dispersal_settlement_layer_confirmation_depth() {
        let config = EigenConfig {
            settlement_layer_confirmation_depth: 5,
            authenticated: true,
            ..EigenConfig::default()
        };
        let secrets = test_secrets();
        let client = EigenClient::new(config.clone(), secrets, Arc::new(MockGetBlobData))
            .await
            .unwrap();
        let data = vec![1; 20];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();
        let blob_info = get_blob_info(&client, &result).await.unwrap();

        let expected_inclusion_data = blob_info.clone().blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);
        let retrieved_data = client.get_blob_data(blob_info).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }
}
