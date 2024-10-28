use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tonic::transport::{Channel, ClientTlsConfig};
use zksync_config::configs::da_client::eigen_da::EigenDAConfig;
use zksync_da_client::{
    types::{self},
    DataAvailabilityClient,
};

use super::disperser_clients::{
    disperser::disperser_client::DisperserClient, memstore::MemStore, remote::RemoteClient,
    Disperser,
};

#[derive(Clone, Debug)]
pub struct EigenDAClient {
    disperser: Disperser,
}

impl EigenDAClient {
    pub const BLOB_SIZE_LIMIT_IN_BYTES: usize = 2 * 1024 * 1024; // 2MB

    pub async fn new(config: EigenDAConfig) -> anyhow::Result<Self> {
        let disperser: Disperser = match config.clone() {
            EigenDAConfig::Disperser(config) => {
                match rustls::crypto::ring::default_provider().install_default() {
                    Ok(_) => {}
                    Err(_) => {} // This is not an actual error, we expect this function to return an Err(Arc<CryptoProvider>)
                };

                let inner = Channel::builder(config.disperser_rpc.parse()?)
                    .tls_config(ClientTlsConfig::new().with_native_roots())?;
                let disperser = Arc::new(Mutex::new(DisperserClient::connect(inner).await?));
                Disperser::Remote(RemoteClient { disperser, config })
            }
            EigenDAConfig::MemStore(config) => Disperser::Memory(MemStore::new(config)),
        };
        Ok(Self { disperser })
    }
}

#[async_trait]
impl DataAvailabilityClient for EigenDAClient {
    async fn dispatch_blob(
        &self,
        _batch_number: u32,
        blob_data: Vec<u8>,
    ) -> Result<types::DispatchResponse, types::DAError> {
        match &self.disperser {
            Disperser::Remote(remote_disperser) => remote_disperser.disperse_blob(blob_data).await,
            Disperser::Memory(memstore) => memstore.clone().store_blob(blob_data).await,
        }
    }

    async fn get_inclusion_data(
        &self,
        blob_id: &str,
    ) -> anyhow::Result<Option<types::InclusionData>, types::DAError> {
        match &self.disperser {
            Disperser::Remote(remote_client) => remote_client.get_inclusion_data(blob_id).await,
            Disperser::Memory(memstore) => memstore.clone().get_inclusion_data(blob_id).await,
        }
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(Self::BLOB_SIZE_LIMIT_IN_BYTES)
    }
}

#[cfg(test)]
impl EigenDAClient {
    pub async fn get_blob_data(
        &self,
        blob_id: &str,
    ) -> anyhow::Result<Option<Vec<u8>>, types::DAError> {
        match &self.disperser {
            Disperser::Remote(remote_client) => remote_client.get_blob_data(blob_id).await,
            Disperser::Memory(memstore) => memstore.clone().get_blob_data(blob_id).await,
        }
    }
}

pub fn to_retriable_error(error: anyhow::Error) -> types::DAError {
    types::DAError {
        error,
        is_retriable: true,
    }
}

pub fn to_non_retriable_error(error: anyhow::Error) -> types::DAError {
    types::DAError {
        error,
        is_retriable: false,
    }
}

#[cfg(test)]
mod test {
    use zksync_config::configs::da_client::eigen_da::{DisperserConfig, MemStoreConfig};

    use super::*;
    use crate::eigen_da::disperser_clients::blob_info::BlobInfo;

    #[tokio::test]
    async fn test_eigenda_memory_disperser() {
        let config = EigenDAConfig::MemStore(MemStoreConfig {
            api_node_url: "".to_string(),
            custom_quorum_numbers: Some(vec![]),
            account_id: Some("".to_string()),
            max_blob_size_bytes: 2 * 1024 * 1024, // 2MB,
            blob_expiration: 60 * 2,
            get_latency: 0,
            put_latency: 0,
        });
        let client = EigenDAClient::new(config).await.unwrap();
        let data = vec![1u8; 100];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();
        let retrieved_data = client.get_inclusion_data(&result.blob_id).await.unwrap();
        assert_eq!(retrieved_data.unwrap().data, data);
    }

    #[tokio::test]
    async fn test_eigenda_remote_disperser_non_authenticated() {
        let config = EigenDAConfig::Disperser(DisperserConfig {
            api_node_url: String::default(),
            custom_quorum_numbers: None,
            account_id: None,
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            eth_confirmation_depth: -1,
            eigenda_eth_rpc: String::default(),
            eigenda_svc_manager_addr: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            blob_size_limit: 2 * 1024 * 1024, // 2MB
            status_query_timeout: 1800,       // 30 minutes
            status_query_interval: 5,         // 5 seconds
            wait_for_finalization: false,
            authenticaded: false,
        });
        let client = EigenDAClient::new(config).await.unwrap();
        let data = vec![1u8; 100];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();

        let blob_info: BlobInfo =
            rlp::decode(&hex::decode(result.blob_id.clone()).unwrap()).unwrap();
        let expected_inclusion_data = blob_info.blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);

        let retrieved_data = client.get_blob_data(&result.blob_id).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }

    #[tokio::test]
    async fn test_eigenda_remote_disperser_authenticated() {
        let config = EigenDAConfig::Disperser(DisperserConfig {
            api_node_url: String::default(),
            custom_quorum_numbers: None,
            account_id: Some(
                "3957dbf2beff0cc8163b8068164502da9d739f22e9922338b178b59406124600".to_string(),
            ),
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            eth_confirmation_depth: -1,
            eigenda_eth_rpc: String::default(),
            eigenda_svc_manager_addr: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            blob_size_limit: 2 * 1024 * 1024, // 2MB
            status_query_timeout: 1800,       // 30 minutes
            status_query_interval: 5,         // 5 seconds
            wait_for_finalization: false,
            authenticaded: true,
        });
        let client = EigenDAClient::new(config).await.unwrap();
        let data = vec![1u8; 100];
        let result = client.dispatch_blob(0, data.clone()).await.unwrap();

        let blob_info: BlobInfo =
            rlp::decode(&hex::decode(result.blob_id.clone()).unwrap()).unwrap();
        let expected_inclusion_data = blob_info.blob_verification_proof.inclusion_proof;
        let actual_inclusion_data = client
            .get_inclusion_data(&result.blob_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(expected_inclusion_data, actual_inclusion_data);

        let retrieved_data = client.get_blob_data(&result.blob_id).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }

    #[tokio::test]
    async fn test_foo() {
        let config = EigenDAConfig::Disperser(DisperserConfig {
            api_node_url: String::default(),
            custom_quorum_numbers: None,
            account_id: None,
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            eth_confirmation_depth: -1,
            eigenda_eth_rpc: String::default(),
            eigenda_svc_manager_addr: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            blob_size_limit: 2 * 1024 * 1024, // 2MB
            status_query_timeout: 1800,       // 30 minutes
            status_query_interval: 5,         // 5 seconds
            wait_for_finalization: false,
            authenticaded: false,
        });
        let client = EigenDAClient::new(config).await.unwrap();
        let data = vec![1u8; 100];
        let blob_id = client.dispatch_blob(0, data.clone()).await.unwrap().blob_id;
        let retrieved_data = client.get_blob_data(&blob_id).await.unwrap();
        assert_eq!(retrieved_data.unwrap(), data);
    }

    #[tokio::test]
    async fn test_bar() {
        let config = EigenDAConfig::Disperser(DisperserConfig {
            api_node_url: String::default(),
            custom_quorum_numbers: None,
            account_id: None,
            disperser_rpc: "https://disperser-holesky.eigenda.xyz:443".to_string(),
            eth_confirmation_depth: -1,
            eigenda_eth_rpc: String::default(),
            eigenda_svc_manager_addr: "0xD4A7E1Bd8015057293f0D0A557088c286942e84b".to_string(),
            blob_size_limit: 2 * 1024 * 1024, // 2MB
            status_query_timeout: 1800,       // 30 minutes
            status_query_interval: 5,         // 5 seconds
            wait_for_finalization: false,
            authenticaded: false,
        });
        let client = EigenDAClient::new(config).await.unwrap();

        let blob_id = "010000f901f5f850f842a025d5eb2ffb96e675f2980f741f32504bcb43c4e34659b2af3c80e606f9d4140fa000e362c18d14d8b58035fb292a1a56209a6cf631efa007aa029fff9fadf5e66d02cac480213701c401213701f901a0830104121cf873eba0d0853277779860119559e258310b8e185068be8d41b39724f19187706afc5f5382000182636483280e69a087c3905755a51ec20c7675cd5056294019ff585ba98ff1b20ca5bb148ee8c2bd0083280ecca0ec861f13db51fdadbb31071078b7b5fde83c0f0a2b2b250b922539efce770113b9012041f22b31df5a60eba60594352e81cec3282134badfd9a826929e8b82c24781b8afa1af956a1575d517dfec94bc405319b2c297fe1f7b68ecf5acca6daabd6f0c68c53c3184cb2f2ff49270edbb3dc104d0eeaaf99b88f6e0827b8a3cfd3693f2ea26212ef77a1143a0a2b047a6eb565fbc3a9edb4b18fea71f2edd03abfaf74c3d7eb4273627024cc4299871c8ee89729fbb67d9f8a6569801e41620df91d356aa448b5f5ac22f01edd17a7404e3c73f89effef38ce41cd1a57c24530b65d8bd308fb91d9cad7001fc00bc85bb45fe404321d6d792a45cef9b026ecf3d8dc90cb74fe6f7bd2c5b9128588db0bc8e639c15c0ed049b748d6d9a48e3b5bf3635c7d7a904fe8bcdcd8042181cad32a00315901969f49e7e577185b30a4cb058eefc820001".to_string();

        let _response = client.get_blob_data(&blob_id).await.unwrap().unwrap();
        // let foo = hex::decode(response).unwrap();
        todo!()
    }
}
