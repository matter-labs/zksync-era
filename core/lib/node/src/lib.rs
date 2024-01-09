use std::{any::Any, collections::HashMap, fmt};

use futures::{future::BoxFuture, FutureExt};
use resources::Resource;
use tokio::{runtime::Runtime, sync::watch};
// Public re-exports from external crate to minimize the required dependencies.
pub use zksync_health_check::{CheckHealth, ReactiveHealthCheck};

pub mod resources;
pub mod tasks;

pub trait IntoZkSyncTask: 'static + Send + Sync {
    type Config: 'static + Send + Sync;

    /// Creates a new task.
    /// Normally, at this step the task is only expected to gather required resources from `ZkSyncNode`.
    ///
    /// If additional preparations are required, they should be done in `before_launch`.
    fn create(
        node: &ZkSyncNode,
        config: Self::Config,
    ) -> Result<Box<dyn ZkSyncTask>, tasks::TaskInitError>;
}

/// A task represents some code that "runs".
/// During its creation, it uses its own config and resources added to the `ZkSyncNode`.
///
/// TODO more elaborate docs
#[async_trait::async_trait]
pub trait ZkSyncTask: 'static + Send + Sync {
    // /// Context that can be shared with the `after_finish` and `after_shutdown`
    // type Context = ();

    /// Gets the healthcheck for the task, if it exists.
    /// Guaranteed to be called only once per task.
    fn healtcheck(&mut self) -> Option<Box<dyn CheckHealth>>;

    /// Runs the task.
    ///
    /// Once any of the task returns, the node will shutdown.
    /// If the task returns an error, the node will spawn an error-level log message and will return a non-zero exit code.
    async fn run(self: Box<Self>) -> anyhow::Result<()>;

    /// TODO: elaborate on sharing state with the hook.
    fn after_finish(&self) -> Option<BoxFuture<'static, ()>> {
        None
    }

    /// TODO: elaborate on sharing state with the hook.
    ///
    /// Asynchronous hook that will be called after *each task* has finished their cleanup.
    /// It is guaranteed that no other task is running at this point, e.g. `ZkSyncNode` will invoke
    /// this hook sequentially for each task.
    ///
    /// This hook can be used to perform some cleanup that assumes exclusive access to the resources, e.g.
    /// to rollback some state.
    fn after_node_shutdown(&self) -> Option<BoxFuture<'static, ()>> {
        None
    }
}

struct TaskRepr {
    task: Option<BoxFuture<'static, anyhow::Result<()>>>,
    after_finish: Option<BoxFuture<'static, ()>>,
    after_node_shutdown: Option<BoxFuture<'static, ()>>,
}

impl fmt::Debug for TaskRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskRepr").finish()
    }
}

/// "Manager" class of the node. Collects all the resources and tasks,
/// then runs tasks until completion.
pub struct ZkSyncNode {
    /// Anything that may be needed by 1 or more tasks to be created.
    /// Think `ConnectionPool`, `TxSender`, `SealManager`, `EthGateway`, `GasAdjuster`.
    /// There is a note on `dyn Any` below the snippet.
    resources: HashMap<String, Box<dyn Any>>,
    /// Named tasks.
    tasks: HashMap<String, TaskRepr>,
    // TODO: healthchecks

    // TODO: circuit breakers
    stop_sender: watch::Sender<bool>,
    runtime: Runtime,
}

impl Default for ZkSyncNode {
    fn default() -> Self {
        Self::new()
    }
}

impl ZkSyncNode {
    pub fn new() -> Self {
        // TODO: Check that tokio context is not available to make sure that every future is spawned on the dedicated runtime?
        // TODO: Provide a way to get runtime handle?

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let (stop_sender, stop_receiver) = watch::channel(false);
        let mut self_ = Self {
            resources: HashMap::new(),
            tasks: HashMap::new(),
            stop_sender,
            runtime,
        };
        self_.add_resource(
            resources::stop_receiver::RESOURCE_NAME,
            resources::stop_receiver::StopReceiverResource::new(stop_receiver),
        );

        self_
    }

    /// Helper utility allowing to execute asynchrnous code within [`ZkSyncTask::new`].
    /// This method is intended to only be used to invoke async contstructors, deferring as much async logic as possible
    /// to [`ZkSyncTask::before_launch`].
    pub fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
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
    pub fn add_task<T: IntoZkSyncTask>(
        &mut self,
        name: impl AsRef<str>,
        config: impl Into<T::Config>,
    ) {
        let task = T::create(self, config.into()).unwrap(); // TODO: Do not unwrap
        let after_finish = task.after_finish();
        let after_node_shutdown = task.after_node_shutdown();
        let name = String::from(name.as_ref());
        let task_future = Box::pin(task.run());
        let task_repr = TaskRepr {
            task: Some(task_future),
            after_finish,
            after_node_shutdown,
        };
        self.tasks.insert(name, task_repr);
    }

    /// Runs the system.
    pub fn run(mut self) -> anyhow::Result<()> {
        let rt_handle = self.runtime.handle().clone();
        let join_handles: Vec<_> = self
            .tasks
            .values_mut()
            .map(|task| {
                let task = task
                    .task
                    .take()
                    .expect("Task must be Some prior to running");
                rt_handle.spawn(task).fuse()
            })
            .collect();

        // Run the tasks until one of them exits.
        let (resolved, _idx, remaining) = self
            .runtime
            .block_on(futures::future::select_all(join_handles));
        let failure = match resolved {
            Ok(Ok(())) => {
                tracing::info!("Task exited successfully"); // TODO which one
                false
            }
            Ok(Err(err)) => {
                tracing::error!("Task exited with an error: {err}"); // TODO which one
                true
            }
            Err(_) => {
                tracing::error!("Task panicked"); // TODO which one
                true
            }
        };

        // Send stop signal to remaining tasks and wait for them to finish.
        self.stop_sender.send(true).ok();
        self.runtime.block_on(futures::future::join_all(remaining));

        // Call after_finish hooks.
        // For this and shutdown hooks we need to use local set, since these futures are not `Send`.
        let local_set = tokio::task::LocalSet::new();
        let join_handles = self.tasks.values_mut().filter_map(|task| {
            task.after_finish
                .take()
                .map(|task| local_set.spawn_local(task))
        });
        local_set.block_on(&self.runtime, futures::future::join_all(join_handles));

        // Call after_node_shutdown hooks.
        let join_handles = self.tasks.values_mut().filter_map(|task| {
            task.after_node_shutdown
                .take()
                .map(|task| local_set.spawn_local(task))
        });
        local_set.block_on(&self.runtime, futures::future::join_all(join_handles));

        if failure {
            anyhow::bail!("Task failed"); // TODO elaborate
        } else {
            Ok(())
        }
    }
}
