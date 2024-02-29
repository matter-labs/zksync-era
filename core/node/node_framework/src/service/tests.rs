use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{anyhow, Context};
use tokio::{runtime::Runtime, sync::watch};

use crate::{
    resource::Resource,
    service::{ServiceContext, StopReceiver, WiringError, WiringLayer, ZkStackService},
    task::Task,
};

/// `ZkStackService::new` method has to have a check for nested runtime.
#[test]
fn test_new_with_nested_runtime() {
    let runtime = Runtime::new().unwrap();

    let initialization_result =
        runtime.block_on(async { ZkStackService::new().unwrap_err().to_string() });

    assert_eq!(
        initialization_result,
        "Detected a Tokio Runtime. ZkStackService manages its own runtime and does not support nested runtimes"
        .to_string()
    );
}

/// `ZkStackService::add_layer` method has to add multiple layers into `self.layers`.
#[test]
fn test_add_layer() {
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
    struct AnotherLayer;

    #[async_trait::async_trait]
    impl WiringLayer for AnotherLayer {
        fn layer_name(&self) -> &'static str {
            "another_layer"
        }

        async fn wire(self: Box<Self>, mut _node: ServiceContext<'_>) -> Result<(), WiringError> {
            Ok(())
        }
    }

    let mut zk_stack_service = ZkStackService::new().unwrap();
    zk_stack_service
        .add_layer(DefaultLayer)
        .add_layer(AnotherLayer);
    let actual_layers_len = zk_stack_service.layers.len();
    assert_eq!(
        2, actual_layers_len,
        "Incorrect number of layers in the service"
    );
}

/// `ZkStackService::run` method has to return error if there is no tasks added.
#[test]
fn test_run_with_no_tasks() {
    let empty_run_result = ZkStackService::new().unwrap().run();
    assert_eq!(
        empty_run_result.unwrap_err().to_string(),
        "No tasks to run".to_string(),
        "Incorrect result for creating a service with no tasks"
    );
}

/// `ZkStackService::run` method has to take into account errors on wiring step.
#[test]
fn test_run_with_error_tasks() {
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

    let mut zk_stack_service = ZkStackService::new().unwrap();
    let error_layer = WireErrorLayer;
    zk_stack_service.add_layer(error_layer);
    let result = zk_stack_service.run();
    assert_eq!(
        result.unwrap_err().to_string(),
        "One or more task weren't able to start".to_string(),
        "Incorrect result for creating a service with wire error layer"
    );
}

/// `ZkStackService::run` method has to take into account errors inside task execution.
#[test]
fn test_run_with_failed_tasks() {
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
        fn name() -> &'static str {
            "error_task"
        }
        async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
            anyhow::bail!("error task")
        }
    }

    let mut zk_stack_service: ZkStackService = ZkStackService::new().unwrap();
    zk_stack_service.add_layer(TaskErrorLayer);
    let result = zk_stack_service.run();
    assert_eq!(
        result.unwrap_err().to_string(),
        "Task error_task failed".to_string(),
        "Incorrect result for creating a service with task that fails"
    );
}

/// Check `ZkStackService::run` method tasks' expected behavior.
#[test]
fn test_task_run() {
    #[derive(Debug)]
    struct TasksLayer {
        successful_task_was_run: Arc<Mutex<bool>>,
        remaining_task_was_run: Arc<Mutex<bool>>,
    }

    #[async_trait::async_trait]
    impl WiringLayer for TasksLayer {
        fn layer_name(&self) -> &'static str {
            "tasks_layer"
        }

        async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
            node.add_task(Box::new(SuccessfulTask(
                self.successful_task_was_run.clone(),
            )))
            .add_task(Box::new(RemainingTask(self.remaining_task_was_run.clone())));
            Ok(())
        }
    }

    /// `ZkStackService::run` method has to run tasks, added to the layer.
    #[derive(Debug)]
    struct SuccessfulTask(Arc<Mutex<bool>>);

    #[async_trait::async_trait]
    impl Task for SuccessfulTask {
        fn name() -> &'static str {
            "successful_task"
        }
        async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
            let mut guard = self.0.lock().unwrap();
            *guard = true;
            Ok(())
        }
    }

    /// `ZkStackService::run` method has to allow remaining tasks to finish,
    /// after stop signal was send.
    #[derive(Debug)]
    struct RemainingTask(Arc<Mutex<bool>>);

    #[async_trait::async_trait]
    impl Task for RemainingTask {
        fn name() -> &'static str {
            "remaining_task"
        }
        async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
            stop_receiver.0.changed().await?;
            let mut guard = self.0.lock().unwrap();
            *guard = true;
            Ok(())
        }
    }

    let successful_task_was_run = Arc::new(Mutex::new(false));
    let remaining_task_was_run = Arc::new(Mutex::new(false));

    let mut zk_stack_service = ZkStackService::new().unwrap();

    zk_stack_service.add_layer(TasksLayer {
        successful_task_was_run: successful_task_was_run.clone(),
        remaining_task_was_run: remaining_task_was_run.clone(),
    });

    assert!(
        zk_stack_service.run().is_ok(),
        "ZkStackService run finished with an error, but it shouldn't"
    );
    let res1 = *successful_task_was_run.lock().unwrap();
    assert!(res1, "Incorrect resource value");

    let res2 = *remaining_task_was_run.lock().unwrap();
    assert!(res2, "Incorrect resource value");
}

/// Checks that setup hook is invoked and the service lifecycle can be altered through it.
#[test]
fn test_setup_hook() -> anyhow::Result<()> {
    #[derive(Debug)]
    struct SetupHookLayer {
        early_sender: watch::Sender<bool>,
        late_sender: watch::Sender<bool>,
    }

    #[async_trait::async_trait]
    impl WiringLayer for SetupHookLayer {
        fn layer_name(&self) -> &'static str {
            "setup_hook_layer"
        }

        async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
            node.add_task(Box::new(EarlyTask {
                launched: self.early_sender,
            }));
            node.add_task(Box::new(LateTask {
                launched: self.late_sender,
            }));
            node.insert_resource(SampleResource)?;
            Ok(())
        }
    }

    #[derive(Debug)]
    struct EarlyTask {
        launched: watch::Sender<bool>,
    }

    #[async_trait::async_trait]
    impl Task for EarlyTask {
        fn name() -> &'static str {
            "early_task"
        }
        async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
            self.launched.send(true)?;
            Ok(())
        }
    }

    #[derive(Debug)]
    struct LateTask {
        launched: watch::Sender<bool>,
    }

    #[async_trait::async_trait]
    impl Task for LateTask {
        fn name() -> &'static str {
            "late_task"
        }
        async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
            self.launched.send(true)?;
            Ok(())
        }
    }

    #[derive(Debug, Clone)]
    struct SampleResource;

    impl Resource for SampleResource {
        fn resource_id() -> crate::resource::ResourceId {
            "sample_resource".into()
        }
    }

    let (early_sender, mut early_receiver) = watch::channel(false);
    let (late_sender, late_receiver) = watch::channel(false);
    let layer = SetupHookLayer {
        early_sender,
        late_sender,
    };

    let mut zk_stack_service = ZkStackService::new().unwrap();
    zk_stack_service.add_layer(layer);

    {
        let late_receiver = late_receiver.clone();
        zk_stack_service.run_with_setup(move |service| {
            Box::pin(async move {
                anyhow::ensure!(
                    !*early_receiver.borrow(),
                    "Early task was launched before setup hook"
                );
                anyhow::ensure!(
                    !*late_receiver.borrow(),
                    "Early task was launched before setup hook"
                );

                // Launch early task.
                service.launch_task(EarlyTask::name())?;
                tokio::time::timeout(Duration::from_millis(200), early_receiver.changed())
                    .await??;
                anyhow::ensure!(
                    *early_receiver.borrow(),
                    "Early task wasn't launched during setup hook"
                );

                // Make sure that resources can be accessed.
                let _ = service
                    .get_resource::<SampleResource>()
                    .context("Unable to get sample resource")?;

                Ok(())
            })
        })?;
    }

    anyhow::ensure!(*late_receiver.borrow(), "Late task wasn't launched");

    Ok(())
}
