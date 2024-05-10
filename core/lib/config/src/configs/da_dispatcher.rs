use crate::ObjectStoreConfig;

#[derive(Clone, Debug)]
pub struct DALayerInfo {
    pub url: String,
    pub private_key: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum DACredentials {
    DALayer(DALayerInfo),
    GCS(ObjectStoreConfig),
}

#[derive(Debug, Clone)]
pub struct DADispatcherConfig {
    pub credentials: DACredentials,
}

impl DADispatcherConfig {
    pub fn for_tests() -> Self {
        Self {
            credentials: DACredentials::DALayer(DALayerInfo {
                url: "http://localhost:1234".to_string(),
                private_key: vec![1, 2, 3],
            }),
        }
    }
}
