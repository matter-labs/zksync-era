use std::sync::Arc;

use tokio::sync::watch;
use zksync_dal::{connection::ConnectionPoolBuilder, ConnectionPool};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};

use crate::l1_gas_price::{L1GasPriceProvider, L1TxParamsProvider};

mod tasks;

#[derive(Default)]
pub struct Resources {
    master_pool: Option<ConnectionPoolBuilder>,
    replica_pool: Option<ConnectionPoolBuilder>,
    prover_pool: Option<ConnectionPoolBuilder>,
    l1_gas_info_provider: Option<Arc<dyn L1GasPriceProvider>>,
    tx_params_provider: Option<Arc<dyn L1TxParamsProvider>>,
    object_store_factory: Option<ObjectStoreFactory>,
    stop_receiver: Option<watch::Receiver<bool>>,
}

impl std::fmt::Debug for Resources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: Provide a better implementation.
        f.debug_struct("Resources").finish()
    }
}

impl Resources {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_master_pool(mut self, master_pool: ConnectionPoolBuilder) -> Self {
        self.master_pool = Some(master_pool);
        self
    }

    pub fn with_replica_pool(mut self, replica_pool: ConnectionPoolBuilder) -> Self {
        self.replica_pool = Some(replica_pool);
        self
    }

    pub fn with_prover_pool(mut self, prover_pool: ConnectionPoolBuilder) -> Self {
        self.prover_pool = Some(prover_pool);
        self
    }

    pub fn with_l1_gas_info_provider(
        mut self,
        l1_gas_info_provider: Arc<dyn L1GasPriceProvider>,
    ) -> Self {
        self.l1_gas_info_provider = Some(l1_gas_info_provider);
        self
    }

    pub fn with_tx_params_provider(
        mut self,
        tx_params_provider: Arc<dyn L1TxParamsProvider>,
    ) -> Self {
        self.tx_params_provider = Some(tx_params_provider);
        self
    }

    pub fn with_object_store_factory(mut self, object_store_factory: ObjectStoreFactory) -> Self {
        self.object_store_factory = Some(object_store_factory);
        self
    }

    pub fn with_stop_receiver(mut self, stop_receiver: watch::Receiver<bool>) -> Self {
        self.stop_receiver = Some(stop_receiver);
        self
    }

    pub async fn master_pool(&self) -> anyhow::Result<ConnectionPool> {
        self.master_pool
            .as_ref()
            .map(|builder| builder.build())
            .expect("master pool is not initialized")
            .await
    }

    pub async fn master_pool_singleton(&self) -> anyhow::Result<ConnectionPool> {
        self.master_pool
            .as_ref()
            .map(|builder| builder.build_singleton())
            .expect("master pool is not initialized")
            .await
    }

    pub async fn replica_pool(&self) -> anyhow::Result<ConnectionPool> {
        self.replica_pool
            .as_ref()
            .map(|builder| builder.build())
            .expect("replica pool is not initialized")
            .await
    }

    pub async fn replica_pool_singleton(&self) -> anyhow::Result<ConnectionPool> {
        self.replica_pool
            .as_ref()
            .map(|builder| builder.build_singleton())
            .expect("replica pool is not initialized")
            .await
    }

    pub async fn prover_pool(&self) -> anyhow::Result<ConnectionPool> {
        self.prover_pool
            .as_ref()
            .map(|builder| builder.build())
            .expect("prover pool is not initialized")
            .await
    }

    pub async fn prover_pool_singleton(&self) -> anyhow::Result<ConnectionPool> {
        self.prover_pool
            .as_ref()
            .map(|builder| builder.build_singleton())
            .expect("prover pool is not initialized")
            .await
    }

    pub fn l1_gas_info_provider(&self) -> Arc<dyn L1GasPriceProvider> {
        self.l1_gas_info_provider
            .as_ref()
            .expect("l1 gas info provider is not initialized")
            .clone()
    }

    pub fn tx_params_provider(&self) -> Arc<dyn L1TxParamsProvider> {
        self.tx_params_provider
            .as_ref()
            .expect("tx params provider is not initialized")
            .clone()
    }

    pub async fn object_store(&self) -> Box<dyn ObjectStore> {
        self.object_store_factory
            .as_ref()
            .map(|factory| factory.create_store())
            .expect("object store factory is not initialized")
            .await
    }

    pub fn stop_receiver(&self) -> watch::Receiver<bool> {
        self.stop_receiver
            .as_ref()
            .expect("stop receiver is not initialized")
            .clone()
    }
}

pub struct ZkSyncNode {}

impl std::fmt::Debug for ZkSyncNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: Provide better impl.
        f.debug_struct("ZkSyncNode").finish()
    }
}
