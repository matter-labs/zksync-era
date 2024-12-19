/// EigenDA Client tests are ignored by default, because they require a remote dependency,
/// which may not always be available, causing tests to be flaky.
/// To run these tests, use the following command:
/// `cargo test -p zksync_da_clients -- --ignored`
#[cfg(test)]
mod tests {
    use std::{str::FromStr, time::Duration};

    use backon::{ConstantBuilder, Retryable};
    use serial_test::serial;
    use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
    use zksync_da_client::{
        types::{DAError, DispatchResponse},
        DataAvailabilityClient,
    };
    use zksync_types::secrets::PrivateKey;

    use crate::eigen::{blob_info::BlobInfo, EigenClient, GetBlobData};

    impl<T: GetBlobData> EigenClient<T> {
        pub async fn get_blob_data(
            &self,
            blob_id: BlobInfo,
        ) -> anyhow::Result<Option<Vec<u8>>, DAError> {
            self.client.get_blob_data(blob_id).await
        }

        pub async fn get_commitment(&self, blob_id: &str) -> anyhow::Result<Option<BlobInfo>> {
            self.client.get_commitment(blob_id).await
        }
    }
    const STATUS_QUERY_TIMEOUT: u64 = 1800000; // 30 minutes
    const STATUS_QUERY_INTERVAL: u64 = 5; // 5 ms

    async fn get_blob_info<T: GetBlobData>(
        client: &EigenClient<T>,
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
        async fn call(&self, _input: &'_ str) -> anyhow::Result<Option<Vec<u8>>> {
            Ok(None)
        }
    }

    #[ignore = "depends on external RPC"]
    #[tokio::test]
    #[serial]
    async fn test_non_auth_dispersal() {
        let config = EigenConfig {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            settlement_layer_confirmation_depth: -1,
            eigenda_eth_rpc: Some("https://ethereum-holesky-rpc.publicnode.com".to_string()),
            eigenda_svc_manager_address: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            wait_for_finalization: false,
            authenticated: false,
            g1_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point".to_string(),
            g2_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2".to_string(),
            chain_id: 17000,
        };
        let secrets = EigenSecrets {
            private_key: PrivateKey::from_str(
                "d08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6",
            )
            .unwrap(),
        };
        let client = EigenClient::new(config.clone(), secrets, Box::new(MockGetBlobData))
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
    #[serial]
    async fn test_auth_dispersal() {
        let config = EigenConfig {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            settlement_layer_confirmation_depth: -1,
            eigenda_eth_rpc: Some("https://ethereum-holesky-rpc.publicnode.com".to_string()),
            eigenda_svc_manager_address: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            wait_for_finalization: false,
            authenticated: true,
            g1_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point".to_string(),
            g2_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2".to_string(),
            chain_id: 17000,
        };
        let secrets = EigenSecrets {
            private_key: PrivateKey::from_str(
                "d08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6",
            )
            .unwrap(),
        };
        let client = EigenClient::new(config.clone(), secrets, Box::new(MockGetBlobData))
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
    #[serial]
    async fn test_wait_for_finalization() {
        let config = EigenConfig {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            wait_for_finalization: true,
            authenticated: true,
            g1_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point".to_string(),
            g2_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2".to_string(),
            settlement_layer_confirmation_depth: 0,
            eigenda_eth_rpc: Some("https://ethereum-holesky-rpc.publicnode.com".to_string()),
            eigenda_svc_manager_address: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            chain_id: 17000,
        };
        let secrets = EigenSecrets {
            private_key: PrivateKey::from_str(
                "d08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6",
            )
            .unwrap(),
        };
        let client = EigenClient::new(config.clone(), secrets, Box::new(MockGetBlobData))
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
    #[serial]
    async fn test_settlement_layer_confirmation_depth() {
        let config = EigenConfig {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            settlement_layer_confirmation_depth: 5,
            eigenda_eth_rpc: Some("https://ethereum-holesky-rpc.publicnode.com".to_string()),
            eigenda_svc_manager_address: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            wait_for_finalization: false,
            authenticated: false,
            g1_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point".to_string(),
            g2_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2".to_string(),
            chain_id: 17000,
        };
        let secrets = EigenSecrets {
            private_key: PrivateKey::from_str(
                "d08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6",
            )
            .unwrap(),
        };
        let client = EigenClient::new(config.clone(), secrets, Box::new(MockGetBlobData))
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
    #[serial]
    async fn test_auth_dispersal_settlement_layer_confirmation_depth() {
        let config = EigenConfig {
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            settlement_layer_confirmation_depth: 5,
            eigenda_eth_rpc: Some("https://ethereum-holesky-rpc.publicnode.com".to_string()),
            eigenda_svc_manager_address: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            wait_for_finalization: false,
            authenticated: true,
            g1_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g1.point".to_string(),
            g2_url: "https://github.com/Layr-Labs/eigenda-proxy/raw/2fd70b99ef5bf137d7bbca3461cf9e1f2c899451/resources/g2.point.powerOf2".to_string(),
            chain_id: 17000,
        };
        let secrets = EigenSecrets {
            private_key: PrivateKey::from_str(
                "d08aa7ae1bb5ddd46c3c2d8cdb5894ab9f54dec467233686ca42629e826ac4c6",
            )
            .unwrap(),
        };
        let client = EigenClient::new(config.clone(), secrets, Box::new(MockGetBlobData))
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
