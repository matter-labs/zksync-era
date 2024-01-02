use std::{any::Any, collections::HashMap};

use futures::future::BoxFuture;
use zksync_dal::{connection::ConnectionPoolBuilder, ConnectionPool};

mod resources;
mod tasks;

#[derive(Debug)]
struct Pools {
    master_pool: Option<ConnectionPoolBuilder>,
    replica_pool: Option<ConnectionPoolBuilder>,
    prover_pool: Option<ConnectionPoolBuilder>,
}

impl Pools {
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
}

/// A task represents some code that "runs".
/// During its creation, it uses its own config and resources added to the `ZkSyncNode`.
#[async_trait::async_trait]
pub trait ZkSyncTask: Send + Sync + 'static {
    type Config;

    async fn new(node: &ZkSyncNode, config: impl Into<Self::Config>) -> Self;
    async fn run(self) -> anyhow::Result<()>;
}

/// "Manager" class of the node. Collects all the resources and tasks,
/// then runs tasks until completion.
#[derive(Default)]
pub struct ZkSyncNode {
    /// Anything that may be needed by 1 or more tasks to be created.
    /// Think `ConnectionPool`, `TxSender`, `SealManager`, `EthGateway`, `GasAdjuster`.
    /// There is a note on `dyn Any` below the snippet.
    resources: HashMap<String, Box<dyn Any>>,
    /// Named tasks.
    tasks: HashMap<String, BoxFuture<'static, anyhow::Result<()>>>,
}

impl ZkSyncNode {
    /// Adds a resource. By default, any resource can be requested by multiple
    /// components, thus `T: Clone`. Think `Arc<U>`.
    pub fn add_resource<T: Any + Clone>(&mut self, name: impl AsRef<str>, resource: T) {
        self.resources
            .insert(name.as_ref().into(), Box::new(resource));
    }

    /// To be called in `ZkSyncTask::new(..)`.
    pub fn get_resource<T: Clone + 'static>(&self, name: impl AsRef<str>) -> Option<T> {
        self.resources
            .get(name.as_ref())
            .and_then(|resource| resource.downcast_ref::<T>())
            .cloned()
    }

    /// Takes care of task creation.
    /// May do some "registration" stuff for reporting purposes.
    pub async fn add_task<T: ZkSyncTask>(
        &mut self,
        name: impl AsRef<str>,
        config: impl Into<T::Config>,
    ) {
        // <- todo should not be async
        let task = T::new(self, config).await;
        let future = Box::pin(task.run()); // <- todo should we create a future here?
        self.tasks.insert(name.as_ref().into(), future);
    }

    /// Runs the system.
    pub async fn run(self) -> anyhow::Result<()> {
        let join_handles: Vec<_> = self
            .tasks
            .into_iter()
            .map(|(_, task)| tokio::spawn(task))
            .collect();
        futures::future::select_all(join_handles).await;
        Ok(())
    }
}
