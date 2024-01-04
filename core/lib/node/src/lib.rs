use std::{any::Any, collections::HashMap};

use futures::future::BoxFuture;
use resources::Resource;
// Public re-exports from external crate to minimize the required dependencies.
pub use zksync_health_check::{CheckHealth, ReactiveHealthCheck};

mod resources;
mod tasks;

/// A task represents some code that "runs".
/// During its creation, it uses its own config and resources added to the `ZkSyncNode`.
///
/// TODO more elaborate docs
#[async_trait::async_trait]
pub trait ZkSyncTask: 'static + Send + Sync {
    type Config: 'static + Send + Sync;

    /// Creates a new task.
    /// Normally, at this step the task is only expected to gather required resources from `ZkSyncNode`.
    ///
    /// If additional preparations are required, they should be done in `before_launch`.
    fn new(node: &ZkSyncNode, config: Self::Config) -> Self;

    /// Gets the healthcheck for the task, if it exists.
    /// Guaranteed to be called only once per task.
    fn healtcheck(&mut self) -> Option<Box<dyn CheckHealth>>;

    /// Asynchronous hook that can be used to prepare the task for launch.
    async fn before_launch(&mut self) {}

    /// Runs the task.
    async fn run(self) -> anyhow::Result<()>;

    /// Asynchronous hook that can be used to perform some actions after the task is finished.
    /// It can be used to perform any sort of cleanup before the application exits.
    async fn after_finish(&mut self) {}
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
    // TODO: healthchecks

    // TODO: circuit breakers
}

impl ZkSyncNode {
    /// Helper utility allowing to execute asynchrnous code within [`ZkSyncTask::new`].
    /// This method is intended to only be used to invoke async contstructors, deferring as much async logic as possible
    /// to [`ZkSyncTask::before_launch`].
    pub fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        // TODO: Use self-stored runtime here.
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    /// Adds a resource. By default, any resource can be requested by multiple
    /// components, thus `T: Clone`. Think `Arc<U>`.
    pub fn add_resource<T: Resource>(&mut self, name: impl AsRef<str>, resource: T) {
        self.resources
            .insert(name.as_ref().into(), Box::new(resource));
    }

    /// To be called in `ZkSyncTask::new(..)`.
    pub fn get_resource<T: Resource>(&self, name: impl AsRef<str>) -> Option<T> {
        self.resources
            .get(name.as_ref())
            .and_then(|resource| resource.downcast_ref::<T>())
            .cloned()
    }

    /// Takes care of task creation.
    /// May do some "registration" stuff for reporting purposes.
    pub fn add_task<T: ZkSyncTask>(&mut self, name: impl AsRef<str>, config: impl Into<T::Config>) {
        // <- todo should not be async
        let task = T::new(self, config.into());
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
