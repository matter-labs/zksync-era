#![allow(dead_code)]

use zksync_node_framework::{IntoContext, Resource, StopReceiver, Task, TaskId};

#[derive(Clone)]
struct ResourceA;

impl Resource for ResourceA {
    fn name() -> String {
        "a".to_string()
    }
}

struct TaskA;

#[async_trait::async_trait]
impl Task for TaskA {
    fn id(&self) -> TaskId {
        "batch_status_updater".into()
    }

    async fn run(self: Box<Self>, _stop_receiver: StopReceiver) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(IntoContext)]
struct SimpleStruct {
    #[context(default)]
    _field: ResourceA,
    #[context(task)]
    _field_2: TaskA,
}

fn main() {}
