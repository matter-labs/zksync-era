#![allow(dead_code)]

use zksync_node_framework::{IntoContext, Resource, StopReceiver, Task, TaskId};

#[derive(Clone)]
struct ResourceA;

impl Resource for ResourceA {
    fn name() -> String {
        "a".to_string()
    }
}

#[derive(Default)]
struct TaskA;

#[async_trait::async_trait]
impl Task for TaskA {
    fn id(&self) -> TaskId {
        "batch_transaction_fetcher".into()
    }

    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(IntoContext)]
struct SimpleStruct {
    _field: ResourceA,
    #[context(task, default)]
    _field_2: TaskA,
}

fn main() {}
