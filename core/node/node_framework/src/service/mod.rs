use std::{collections::HashMap, fmt};

use futures::{future::BoxFuture, FutureExt};
use tokio::{runtime::Runtime, sync::watch};

pub use self::{context::ServiceContext, stop_receiver::StopReceiver};
use crate::{
    resource::{ResourceId, StoredResource},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

mod context;
mod stop_receiver;

/// "Manager" class for a set of tasks. Collects all the resources and tasks,
/// then runs tasks until completion.
///
/// Initialization flow:
/// - Service instance is created with access to the resource provider.
/// - Wiring layers are added to the service. At this step, tasks are not created yet.
/// - Once the `run` method is invoked, service
///   - invokes a `wire` method on each added wiring layer. If any of the layers fails,
///     the service will return an error. If no layers have added a task, the service will
///     also return an error.
///   - waits for any of the tasks to finish.
///   - sends stop signal to all the tasks.
///   - waits for the remaining tasks to finish.
///   - calls `after_node_shutdown` hook for every task that has provided it.
///   - returns the result of the task that has finished.
pub struct ZkStackService {
    /// Cache of resources that have been requested at least by one task.
    resources: HashMap<ResourceId, Box<dyn StoredResource>>,
    /// List of wiring layers.
    layers: Vec<Box<dyn WiringLayer>>,
    /// Tasks added to the service.
    tasks: Vec<Box<dyn Task>>,

    /// Sender used to stop the tasks.
    stop_sender: watch::Sender<bool>,
    /// Tokio runtime used to spawn tasks.
    runtime: Runtime,
}

impl fmt::Debug for ZkStackService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ZkStackService").finish_non_exhaustive()
    }
}

impl ZkStackService {
    pub fn new() -> anyhow::Result<Self> {
        if tokio::runtime::Handle::try_current().is_ok() {
            anyhow::bail!(
                "Detected a Tokio Runtime. ZkStackService manages its own runtime and does not support nested runtimes"
            );
        }
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();

        let (stop_sender, _stop_receiver) = watch::channel(false);
        let self_ = Self {
            resources: HashMap::default(),
            layers: Vec::new(),
            tasks: Vec::new(),
            stop_sender,
            runtime,
        };

        Ok(self_)
    }

    /// Adds a wiring layer.
    /// During the [`run`](ZkStackService::run) call the service will invoke
    /// `wire` method of every layer in the order they were added.
    pub fn add_layer<T: WiringLayer>(&mut self, layer: T) -> &mut Self {
        self.layers.push(Box::new(layer));
        self
    }

    /// Runs the system.
    pub fn run(mut self) -> anyhow::Result<()> {
        // Initialize tasks.
        let wiring_layers = std::mem::take(&mut self.layers);

        let mut errors: Vec<(String, WiringError)> = Vec::new();

        let runtime_handle = self.runtime.handle().clone();
        for layer in wiring_layers {
            let name = layer.layer_name().to_string();
            let task_result =
                runtime_handle.block_on(layer.wire(ServiceContext::new(&name, &mut self)));
            if let Err(err) = task_result {
                // We don't want to bail on the first error, since it'll provide worse DevEx:
                // People likely want to fix as much problems as they can in one go, rather than have
                // to fix them one by one.
                errors.push((name, err));
                continue;
            };
        }

        // Report all the errors we've met during the init.
        if !errors.is_empty() {
            for (task, error) in errors {
                tracing::error!("Task {task} can't be initialized: {error}");
            }
            anyhow::bail!("One or more task weren't able to start");
        }

        let mut tasks = Vec::new();
        for task in std::mem::take(&mut self.tasks) {
            let name = task.name().to_string();
            let after_node_shutdown = task.after_node_shutdown();
            let task_future = Box::pin(task.run(self.stop_receiver()));
            let task_repr = TaskRepr {
                name,
                task: Some(task_future),
                after_node_shutdown,
            };
            tasks.push(task_repr);
        }
        if tasks.is_empty() {
            anyhow::bail!("No tasks to run");
        }

        // Wiring is now complete.
        for resource in self.resources.values_mut() {
            resource.stored_resource_wired();
        }

        // Prepare tasks for running.
        let rt_handle = self.runtime.handle().clone();
        let join_handles: Vec<_> = tasks
            .iter_mut()
            .map(|task| {
                let task = task.task.take().expect(
                    "Tasks are created by the node and must be Some prior to calling this method",
                );
                rt_handle.spawn(task).fuse()
            })
            .collect();

        // Run the tasks until one of them exits.
        // TODO (QIT-24): wrap every task into a timeout to prevent hanging.
        let (resolved, idx, remaining) = self
            .runtime
            .block_on(futures::future::select_all(join_handles));
        let task_name = tasks[idx].name.clone();
        let failure = match resolved {
            Ok(Ok(())) => {
                tracing::info!("Task {task_name} completed");
                false
            }
            Ok(Err(err)) => {
                tracing::error!("Task {task_name} exited with an error: {err}");
                true
            }
            Err(_) => {
                tracing::error!("Task {task_name} panicked");
                true
            }
        };

        // Send stop signal to remaining tasks and wait for them to finish.
        // Given that we are shutting down, we do not really care about returned values.
        self.stop_sender.send(true).ok();
        self.runtime.block_on(futures::future::join_all(remaining));

        // Call after_node_shutdown hooks.
        let local_set = tokio::task::LocalSet::new();
        let join_handles = tasks.iter_mut().filter_map(|task| {
            task.after_node_shutdown
                .take()
                .map(|task| local_set.spawn_local(task))
        });
        local_set.block_on(&self.runtime, futures::future::join_all(join_handles));

        if failure {
            anyhow::bail!("Task {task_name} failed");
        } else {
            Ok(())
        }
    }

    pub(crate) fn stop_receiver(&self) -> StopReceiver {
        StopReceiver(self.stop_sender.subscribe())
    }
}

struct TaskRepr {
    name: String,
    task: Option<BoxFuture<'static, anyhow::Result<()>>>,
    after_node_shutdown: Option<BoxFuture<'static, ()>>,
}

impl fmt::Debug for TaskRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TaskRepr")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use anyhow::anyhow;
    use futures::lock::Mutex;

    use super::*;

    #[derive(Debug)]
    struct DefaultLayer;

    #[async_trait::async_trait]
    impl WiringLayer for DefaultLayer {
        fn layer_name(&self) -> &'static str {
            "default_layer"
        }

        async fn wire(self: Box<Self>, mut _node: ServiceContext<'_>) -> Result<(), WiringError> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct WireErrorLayer;

    #[async_trait::async_trait]
    impl WiringLayer for WireErrorLayer {
        fn layer_name(&self) -> &'static str {
            "wire_error_layer"
        }

        async fn wire(self: Box<Self>, _node: ServiceContext<'_>) -> Result<(), WiringError> {
            Err(WiringError::Internal(anyhow!("wiring error")))
        }
    }

    #[derive(Debug)]
    struct TaskErrorLayer;

    #[async_trait::async_trait]
    impl WiringLayer for TaskErrorLayer {
        fn layer_name(&self) -> &'static str {
            "task_error_layer"
        }

        async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
            node.add_task(Box::new(ErrorTask));
            Ok(())
        }
    }

    #[derive(Debug)]
    struct ErrorTask;

    #[async_trait::async_trait]
    impl Task for ErrorTask {
        fn name(&self) -> &'static str {
            "error_task"
        }
        async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
            anyhow::bail!("error task")
        }
    }

    #[derive(Debug)]
    struct OkTaskLayerOne(Arc<Mutex<bool>>);

    #[async_trait::async_trait]
    impl WiringLayer for OkTaskLayerOne {
        fn layer_name(&self) -> &'static str {
            "ok_task_layer_1"
        }

        async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
            node.add_task(Box::new(OkTaskOne(self.0.clone())));
            Ok(())
        }
    }

    #[derive(Debug)]
    struct OkTaskLayerTwo(Arc<Mutex<bool>>);

    #[async_trait::async_trait]
    impl WiringLayer for OkTaskLayerTwo {
        fn layer_name(&self) -> &'static str {
            "ok_task_layer_2"
        }

        async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
            node.add_task(Box::new(OkTaskTwo(self.0.clone())));
            Ok(())
        }
    }

    #[derive(Debug)]
    struct TimeoutTaskLayer;

    #[async_trait::async_trait]
    impl WiringLayer for TimeoutTaskLayer {
        fn layer_name(&self) -> &'static str {
            "timeout_layer"
        }

        async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
            node.add_task(Box::new(TimeoutTask));
            Ok(())
        }
    }

    #[derive(Debug)]
    struct TimeoutTask;

    #[async_trait::async_trait]
    impl Task for TimeoutTask {
        fn name(&self) -> &'static str {
            "timeout_task"
        }
        async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            Ok(())
        }
    }

    #[derive(Debug)]
    struct OkTaskOne(Arc<Mutex<bool>>);

    #[async_trait::async_trait]
    impl Task for OkTaskOne {
        fn name(&self) -> &'static str {
            "ok_task_1"
        }
        async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
            loop {
                if *stop_receiver.0.borrow() {
                    let mut guard = self.0.lock().await;
                    *guard = true;
                    break;
                }
            }
            Ok(())
        }
    }

    #[derive(Debug)]
    struct OkTaskTwo(Arc<Mutex<bool>>);

    #[async_trait::async_trait]
    impl Task for OkTaskTwo {
        fn name(&self) -> &'static str {
            "ok_task_2"
        }
        async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
            loop {
                if *stop_receiver.0.borrow() {
                    let mut guard = self.0.lock().await;
                    *guard = false;
                    break;
                }
                let mut guard = self.0.lock().await;
                *guard = true;
            }
            Ok(())
        }
    }

    #[test]
    fn test_new_with_nested_runtime() {
        let runtime = Runtime::new().unwrap();

        let _ = runtime.block_on(async {
            assert!(ZkStackService::new().is_err());
        });
    }

    #[test]
    fn test_add_layer() {
        let mut zk_stack_service = ZkStackService::new().unwrap();
        zk_stack_service
            .add_layer(DefaultLayer)
            .add_layer(TimeoutTaskLayer);
        assert_eq!(2, zk_stack_service.layers.len());
    }

    #[test]
    fn test_zk_stack_service_run_failed_run() {
        // case 1
        assert_eq!(
            ZkStackService::new()
                .unwrap()
                .run()
                .unwrap_err()
                .to_string(),
            "No tasks to run".to_string()
        );

        // case 2
        let mut zk_stack_service_1 = ZkStackService::new().unwrap();
        let error_layer = WireErrorLayer;
        zk_stack_service_1.add_layer(error_layer);
        let res = zk_stack_service_1.run();
        assert_eq!(
            res.unwrap_err().to_string(),
            "One or more task weren't able to start".to_string()
        );

        // case 3
        let mut zk_stack_service_2 = ZkStackService::new().unwrap();
        zk_stack_service_2.add_layer(TaskErrorLayer);
        let res = zk_stack_service_2.run();
        assert_eq!(
            res.unwrap_err().to_string(),
            "Task error_task failed".to_string()
        );
    }

    #[test]
    fn test_zk_stack_service_successful_run() {
        // case 1:
        // check that task is running
        let resource_1 = Arc::new(Mutex::new(false));
        let mut zk_stack_service_1 = ZkStackService::new().unwrap();
        zk_stack_service_1
            .add_layer(OkTaskLayerOne(resource_1.clone()))
            .add_layer(TimeoutTaskLayer);

        assert!(zk_stack_service_1.run().is_ok());
        // task changed resource_1 value to true
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { assert!(*resource_1.lock().await) });

        // case 2:
        // check that after stop signal we still run remaining tasks
        let resource_2 = Arc::new(Mutex::new(false));
        let mut zk_stack_service_2 = ZkStackService::new().unwrap();
        zk_stack_service_2
            .add_layer(OkTaskLayerTwo(resource_2.clone()))
            .add_layer(TimeoutTaskLayer);

        assert!(zk_stack_service_2.run().is_ok());
        // since in the first case above we checked that task updates resource successfully,
        // now we can check that task update resource_2 again to false value after stop signal
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async { assert!(!*resource_2.lock().await) });
    }

    #[test]
    // check that `stop_receiver()` method returns new Receiver for existing sender,
    // and not arbitrary one.
    fn test_stop_receiver() {
        let zk_stack_service = ZkStackService::new().unwrap();
        let _receiver1 = zk_stack_service.stop_sender.subscribe();
        let _receiver2 = zk_stack_service.stop_receiver();
        assert_eq!(2, zk_stack_service.stop_sender.receiver_count());
    }
}
