//! This example is a showcase of the framework usage.
//! It demonstrates the main abstractions of a framework and shows how to use them.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use zksync_node_framework::{
    resource::Resource,
    service::{StopReceiver, ZkStackServiceBuilder},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// This will be an example of a shared resource. Basically, something that can be used by multiple
/// tasks. In a real world, imagine that we have a `ConnectionPool` instead of this structure.
/// Or `GasAdjuster`.
#[derive(Debug, Clone)]
struct MemoryDatabase {
    data: Arc<Mutex<HashMap<String, String>>>,
}

/// Often, we don't want to use a concrete resource. Instead, we want to be able
/// to customize the behavior for whoever uses it. For that, it's often useful to hide
/// the implementation behind a trait. This way it's the code that composes the service that
/// gets to decide how the resource is used.
///
/// Examples of such behavior in a real world: locally we store artifacts in a local storage,
/// but in real envs we use GCP. Alternatively, we have different resource implementations for
/// main node and EN, like `MempoolIO` and `ExternalIO`.
///
/// Whether it makes sense to hide the actual resource behind a trait often depends on the resource
/// itself. For example, our DAL is massive and cannot realistically be changed easily, so it's OK
/// for it to be a concrete resource. But for anything that may realistically have two different
/// implementations, it's often a good idea to hide it behind a trait.
trait Database: 'static + Send + Sync {
    fn put(&self, key: String, value: String);
    fn get(&self, key: String) -> Option<String>;
}

impl Database for MemoryDatabase {
    fn put(&self, key: String, value: String) {
        self.data.lock().unwrap().insert(key, value);
    }

    fn get(&self, key: String) -> Option<String> {
        self.data.lock().unwrap().get(&key).cloned()
    }
}

/// An idiomatic way to create a resource is to prepare a wrapper for it.
/// This way we separate the logic of the framework (which is primarily about glueing things together)
/// from an actual logic of the resource.
#[derive(Clone)]
struct DatabaseResource(pub Arc<dyn Database>);

/// Resource is mostly a marker trait for things that can be added and fetched from the service.
/// It requires things to be `Send`/`Sync` to share them across threads, `'static` (i.e. not bound
/// to any non-static lifetime) and also `Clone`, since any task may receive their own copy of a
/// resource.
///
/// For the latter requirement, there exists an `Unique` wrapper that can be used to store non-`Clone`
/// resources. It's not used in this example, but it's a useful thing to know about.
impl Resource for DatabaseResource {
    fn name() -> String {
        // The convention for resource names is `<scope>/<name>`. In this case, the scope is `common`, but
        // for anything that is component-specific it could have been e.g. `state_keeper` or `api`.
        "common/database".into()
    }
}

/// Now that we have a resource, we can create a task that uses it.
/// This task will be putting values to the database.
struct PutTask {
    // This is a resource that the task will use.
    db: Arc<dyn Database>,
}

impl PutTask {
    async fn run_inner(self) {
        let mut counter = 0;
        loop {
            let key = format!("key_{}", counter);
            let value = format!("value_{}", counter);
            tracing::info!("Put key-value pair: {} -> {}", key, value);
            self.db.put(key, value);
            counter += 1;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

#[async_trait::async_trait]
impl Task for PutTask {
    fn id(&self) -> TaskId {
        // Task names simply have to be unique. They are used for logging and debugging.
        "put_task".into()
    }

    /// This method will be invoked by the framework when the task is started.
    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        tracing::info!("Starting the task {}", self.id());

        // We have to respect the stop receiver and should exit as soon as we receive
        // a stop request.
        tokio::select! {
            _ = self.run_inner() => {},
            _ = stop_receiver.0.changed() => {},
        }

        Ok(())
    }
}

/// A second task that will be checking the contents of the database and use the same shared resource.
struct CheckTask {
    db: Arc<dyn Database>,
}

impl CheckTask {
    async fn run_inner(self) {
        let mut counter = 0;
        loop {
            let key = format!("key_{}", counter);
            let value = self.db.get(key.clone());
            tracing::info!("Check key-value pair: {} -> {:?}", key, value);
            if value.is_some() {
                counter += 1;
            }
            tokio::time::sleep(Duration::from_millis(800)).await;
        }
    }
}

#[async_trait::async_trait]
impl Task for CheckTask {
    fn id(&self) -> TaskId {
        "check_task".into()
    }

    async fn run(self: Box<Self>, mut stop_receiver: StopReceiver) -> anyhow::Result<()> {
        tracing::info!("Starting the task {}", self.id());

        tokio::select! {
            _ = self.run_inner() => {},
            _ = stop_receiver.0.changed() => {},
        }

        Ok(())
    }
}

/// Now we have to somehow add the resource and tasks to the service.
/// For that, there is a concept in the framework called `WiringLayer`.
/// It's a way to logically group a set of related tasks and resources in a composable way.
/// In real world, you usually want to extract shareable resources into their own wiring layer,
/// and keep tasks separately. Here we do exactly that: we'll use one layer to add a database
/// and another layer to fetch it. The benefit here is that if you want to swap the database
/// implementation, you only have to inject a different wiring layer for database, and the
/// wiring layers for the tasks will remain unchanged.
///
/// Each wiring layer has to implement the `WiringLayer` trait.
/// It will receive its inputs and has to produce outputs, which will be stored in the node.
/// Added resources will be available for the layers that are added after this one,
/// and added tasks will be launched once the wiring completes.
///
/// Inputs and outputs for the layers are defined by the [`FromContext`] and [`IntoContext`]
/// traits correspondingly. These traits have a few ready implementations, for example:
///
/// - `()` can be used if you don't need inputs or don't produce outputs
/// - Any type `T` or `Option<T>` that implements `Resource` also implements both [`FromContext`]
///   and [`IntoContext`]. This can be handy if you work with a single resource.
/// - Otherwise, the most convenient way is to define a struct that will hold all the inputs/ouptuts
///   and derive [`FromContext`] and [`IntoContext`] for it.
///
/// See the trait documentation for more detail.
struct DatabaseLayer;

/// Here we use a derive macro to define outputs for our layer.
#[derive(IntoContext)]
struct DatabaseLayerOutput {
    pub db: DatabaseResource,
}

#[async_trait::async_trait]
impl WiringLayer for DatabaseLayer {
    // We don't need any input for this layer.
    type Input = ();
    // We will produce a database resource.
    type Output = DatabaseLayerOutput;

    fn layer_name(&self) -> &'static str {
        "database_layer"
    }

    /// `wire` method will be invoked by the service before the tasks are started.
    async fn wire(self, _input: Self::Input) -> Result<Self::Output, WiringError> {
        let database = Arc::new(MemoryDatabase {
            data: Arc::new(Mutex::new(HashMap::new())),
        });
        // We add the resource to the service context. This way it will be available for the tasks.
        Ok(DatabaseLayerOutput {
            db: DatabaseResource(database),
        })
    }
}

/// Layer where we add tasks.
struct TasksLayer;

#[derive(FromContext)]
struct TasksLayerInput {
    pub db: DatabaseResource,
}

#[derive(IntoContext)]
struct TasksLayerOutput {
    // Note that when using derive macros, all the fields are assumed to be resources by default.
    // If you want to add a task, you need to apply a special attribute on the field.
    #[context(task)]
    pub put_task: PutTask,
    #[context(task)]
    pub check_task: CheckTask,
}

#[async_trait::async_trait]
impl WiringLayer for TasksLayer {
    // Here we both receive input and produce output.
    type Input = TasksLayerInput;
    type Output = TasksLayerOutput;

    fn layer_name(&self) -> &'static str {
        "tasks_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        // We received the database resource from the context as `input`.
        // Note that we don't really care where it comes from or what's the actual implementation is.
        let db = input.db.0;
        let put_task = PutTask { db: db.clone() };
        let check_task = CheckTask { db };
        // These tasks will be launched by the service once the wiring process is complete.
        Ok(TasksLayerOutput {
            put_task,
            check_task,
        })
    }
}

fn main() -> anyhow::Result<()> {
    let mut builder = ZkStackServiceBuilder::new()?;

    builder.add_layer(DatabaseLayer).add_layer(TasksLayer);

    builder.build().run(None)?;
    Ok(())
}
