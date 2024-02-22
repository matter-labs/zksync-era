use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use tokio::runtime::Runtime;

use crate::service::{
    ServiceContext, StopReceiver, Task, WiringError, WiringLayer, ZkStackService,
};

// ZkStack Service's `new()` method has to have a check for nested runtime.
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

// ZkStack Service's `add_layer()` method has to add multiple layers into `self.layers`.
#[test]
fn test_add_layer() {
    let mut zk_stack_service = ZkStackService::new().unwrap();
    zk_stack_service
        .add_layer(DefaultLayer)
        .add_layer(DefaultLayer);
    let actual_layers_len = zk_stack_service.layers.len();
    assert_eq!(
        2, actual_layers_len,
        "Incorrect number of layers in the service"
    );
}

// ZkStack Service's `run()` method has to return error if there is no tasks added.
#[test]
fn test_run_with_no_tasks() {
    let empty_run_result = ZkStackService::new().unwrap().run();
    assert_eq!(
        empty_run_result.unwrap_err().to_string(),
        "No tasks to run".to_string(),
        "Incorrect result for creating a service with no tasks"
    );
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

// ZkStack Service's `run()` method has to take into account errors on wiring step.
#[test]
fn test_run_with_error_tasks() {
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

// ZkStack Service's `run()` method has to take into account errors on wiring step.
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

// ZkStack Service's `run()` method has to take into account errors inside task execution.
#[test]
fn test_run_with_failed_tasks() {
    let mut zk_stack_service: ZkStackService = ZkStackService::new().unwrap();
    zk_stack_service.add_layer(TaskErrorLayer);
    let result = zk_stack_service.run();
    assert_eq!(
        result.unwrap_err().to_string(),
        "Task error_task failed".to_string(),
        "Incorrect result for creating a service with task that fails"
    );
}

// Layer that adds a task that updates resource and returns
#[derive(Debug)]
struct SuccessfulTaskLayer(Arc<Mutex<bool>>);

#[async_trait::async_trait]
impl WiringLayer for SuccessfulTaskLayer {
    fn layer_name(&self) -> &'static str {
        "successful_task_layer"
    }

    async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
        node.add_task(Box::new(SuccessfulTask(self.0.clone())));
        Ok(())
    }
}

#[derive(Debug)]
struct SuccessfulTask(Arc<Mutex<bool>>);

#[async_trait::async_trait]
impl Task for SuccessfulTask {
    fn name(&self) -> &'static str {
        "successful_task"
    }
    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let mut guard = self.0.lock().unwrap();
        *guard = true;
        Ok(())
    }
}

// Layer for task that updates resource and returns only after stop signal received
#[derive(Debug)]
struct RemainingTaskLayer(Arc<Mutex<bool>>);

#[async_trait::async_trait]
impl WiringLayer for RemainingTaskLayer {
    fn layer_name(&self) -> &'static str {
        "remaining_task_layer"
    }

    async fn wire(self: Box<Self>, mut node: ServiceContext<'_>) -> Result<(), WiringError> {
        node.add_task(Box::new(RemainingTask(self.0.clone())));
        Ok(())
    }
}

#[derive(Debug)]
struct RemainingTask(Arc<Mutex<bool>>);

#[async_trait::async_trait]
impl Task for RemainingTask {
    fn name(&self) -> &'static str {
        "remaining_task"
    }
    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        stop_receiver.0.changed().await?;
        let mut guard = self.0.lock().unwrap();
        *guard = false;
        Ok(())
    }
}

// Check ZkStack Service's `run()` method expected behavior.
// There are 2 tests, one for successful task run and another for remaining task run.
// Second test case relies on the first one.
#[test]
fn test_task_run() {
    // case 1: run the task and check it updates resource successfully before stop signal received.
    let some_bool_resource = Arc::new(Mutex::new(false));
    let mut zk_stack_service = ZkStackService::new().unwrap();
    zk_stack_service.add_layer(SuccessfulTaskLayer(some_bool_resource.clone()));

    assert!(
        zk_stack_service.run().is_ok(),
        "ZkStackService run failed, but it shouldn't"
    );
    let actual_resource_value = *some_bool_resource.lock().unwrap();
    // task changed resource value to true
    assert!(actual_resource_value, "Incorrect resource value");

    // case 2: the idea is to check we are still running remaining tasks after stop signal received.
    // The flow is that successful_task updates resource to true (as checked in the case 1 above)
    // and remaining_task updates it back to false.
    let some_bool_resource = Arc::new(Mutex::new(false));
    let mut zk_stack_service = ZkStackService::new().unwrap();
    zk_stack_service
        .add_layer(SuccessfulTaskLayer(some_bool_resource.clone()))
        .add_layer(RemainingTaskLayer(some_bool_resource.clone()));

    assert!(
        zk_stack_service.run().is_ok(),
        "ZkStackService run failed, but it shouldn't"
    );
    let actual_resource_value = *some_bool_resource.lock().unwrap();
    // As notes above, successful_task updated resource to true and then remaining_task updated it back
    // to false after stop signal received.
    assert!(!actual_resource_value, "Incorrect resource value");
}
