use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use assert_matches::assert_matches;
use tokio::{runtime::Runtime, sync::Barrier};

use crate::{
    service::{StopReceiver, WiringError, WiringLayer, ZkStackServiceBuilder, ZkStackServiceError},
    task::{Task, TaskId},
    IntoContext,
};

// `ZkStack` Service's `new()` method has to have a check for nested runtime.
#[test]
fn test_new_with_nested_runtime() {
    let runtime = Runtime::new().unwrap();

    let initialization_result =
        runtime.block_on(async { ZkStackServiceBuilder::new().unwrap_err() });

    assert_matches!(initialization_result, ZkStackServiceError::RuntimeDetected);
}

#[derive(Debug)]
struct DefaultLayer {
    name: &'static str,
}

#[async_trait::async_trait]
impl WiringLayer for DefaultLayer {
    type Input = ();
    type Output = ();

    fn layer_name(&self) -> &'static str {
        self.name
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        Ok(())
    }
}

// `add_layer` should add multiple layers.
#[test]
fn test_add_layer() {
    let mut zk_stack_service = ZkStackServiceBuilder::new().unwrap();
    zk_stack_service
        .add_layer(DefaultLayer {
            name: "first_layer",
        })
        .add_layer(DefaultLayer {
            name: "second_layer",
        });
    let actual_layers_len = zk_stack_service.layers.len();
    assert_eq!(
        2, actual_layers_len,
        "Incorrect number of layers in the service"
    );
}

// `add_layer` should ignore already added layers.
#[test]
fn test_layers_are_unique() {
    let mut zk_stack_service = ZkStackServiceBuilder::new().unwrap();
    zk_stack_service
        .add_layer(DefaultLayer {
            name: "default_layer",
        })
        .add_layer(DefaultLayer {
            name: "default_layer",
        });
    let actual_layers_len = zk_stack_service.layers.len();
    assert_eq!(
        1, actual_layers_len,
        "Incorrect number of layers in the service"
    );
}

// `ZkStack` Service's `run()` method has to return error if there is no tasks added.
#[test]
fn test_run_with_no_tasks() {
    let empty_run_result = ZkStackServiceBuilder::new().unwrap().build().run(None);
    assert_matches!(empty_run_result.unwrap_err(), ZkStackServiceError::NoTasks);
}

#[derive(Debug)]
struct WireErrorLayer;

#[async_trait::async_trait]
impl WiringLayer for WireErrorLayer {
    type Input = ();
    type Output = ();

    fn layer_name(&self) -> &'static str {
        "wire_error_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        Err(WiringError::Internal(anyhow!("wiring error")))
    }
}

// `ZkStack` Service's `run()` method has to take into account errors on wiring step.
#[test]
fn test_run_with_error_tasks() {
    let mut zk_stack_service = ZkStackServiceBuilder::new().unwrap();
    let error_layer = WireErrorLayer;
    zk_stack_service.add_layer(error_layer);
    let result = zk_stack_service.build().run(None);
    assert_matches!(result.unwrap_err(), ZkStackServiceError::Wiring(_));
}

// `ZkStack` Service's `run()` method has to take into account errors on wiring step.
#[derive(Debug)]
struct TaskErrorLayer;

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
struct TaskErrorLayerOutput {
    #[context(task)]
    task: ErrorTask,
}

#[async_trait::async_trait]
impl WiringLayer for TaskErrorLayer {
    type Input = ();
    type Output = TaskErrorLayerOutput;

    fn layer_name(&self) -> &'static str {
        "task_error_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        Ok(TaskErrorLayerOutput { task: ErrorTask })
    }
}

#[derive(Debug)]
struct ErrorTask;

#[async_trait::async_trait]
impl Task for ErrorTask {
    fn id(&self) -> TaskId {
        "error_task".into()
    }
    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        anyhow::bail!("error task")
    }
}

// `ZkStack` Service's `run()` method has to take into account errors inside task execution.
#[test]
fn test_run_with_failed_tasks() {
    let mut zk_stack_service: ZkStackServiceBuilder = ZkStackServiceBuilder::new().unwrap();
    zk_stack_service.add_layer(TaskErrorLayer);
    let result = zk_stack_service.build().run(None);
    assert_matches!(result.unwrap_err(), ZkStackServiceError::Task(_));
}

#[derive(Debug)]
struct TasksLayer {
    successful_task_was_run: Arc<Mutex<bool>>,
    remaining_task_was_run: Arc<Mutex<bool>>,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
struct TasksLayerOutput {
    #[context(task)]
    successful_task: SuccessfulTask,
    #[context(task)]
    remaining_task: RemainingTask,
}

#[async_trait::async_trait]
impl WiringLayer for TasksLayer {
    type Input = ();
    type Output = TasksLayerOutput;

    fn layer_name(&self) -> &'static str {
        "tasks_layer"
    }

    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let barrier = Arc::new(Barrier::new(2));
        let successful_task = SuccessfulTask(barrier.clone(), self.successful_task_was_run.clone());
        let remaining_task = RemainingTask(barrier, self.remaining_task_was_run.clone());
        Ok(TasksLayerOutput {
            successful_task,
            remaining_task,
        })
    }
}

// `ZkStack` Service's `run()` method has to run tasks, added to the layer.
#[derive(Debug)]
struct SuccessfulTask(Arc<Barrier>, Arc<Mutex<bool>>);

#[async_trait::async_trait]
impl Task for SuccessfulTask {
    fn id(&self) -> TaskId {
        "successful_task".into()
    }
    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.0.wait().await;
        let mut guard = self.1.lock().unwrap();
        *guard = true;
        Ok(())
    }
}

// `ZkStack` Service's `run()` method has to allow remaining tasks to finish,
// after a stop request was sent.
#[derive(Debug)]
struct RemainingTask(Arc<Barrier>, Arc<Mutex<bool>>);

#[async_trait::async_trait]
impl Task for RemainingTask {
    fn id(&self) -> TaskId {
        "remaining_task".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.0.wait().await;
        stop_receiver.0.changed().await?;
        let mut guard = self.1.lock().unwrap();
        *guard = true;
        Ok(())
    }
}

// Check `ZkStack` Service's `run()` method tasks' expected behavior.
#[test]
fn test_task_run() {
    let successful_task_was_run = Arc::new(Mutex::new(false));
    let remaining_task_was_run = Arc::new(Mutex::new(false));

    let mut zk_stack_service = ZkStackServiceBuilder::new().unwrap();

    zk_stack_service.add_layer(TasksLayer {
        successful_task_was_run: successful_task_was_run.clone(),
        remaining_task_was_run: remaining_task_was_run.clone(),
    });

    assert!(
        zk_stack_service.build().run(None).is_ok(),
        "ZkStackServiceBuilder run finished with an error, but it shouldn't"
    );
    let res1 = *successful_task_was_run.lock().unwrap();
    assert!(res1, "Incorrect resource value");

    let res2 = *remaining_task_was_run.lock().unwrap();
    assert!(res2, "Incorrect resource value");
}
