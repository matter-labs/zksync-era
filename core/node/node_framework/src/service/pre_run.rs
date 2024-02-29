use std::{collections::HashMap, fmt};

use futures::{future::Fuse, FutureExt};
use tokio::task::JoinHandle;

use crate::{
    resource::{Resource, ResourceId, StoredResource},
    service::context::downcast_clone,
};

use super::TaskRepr;

/// The service state available right before the service is started.
/// This means that all the resources and tasks are already added, but no tasks are running yet.
/// This state can be used to launch tasks and retrieve resources during the custom service setup.
pub struct PreRun {
    pub(super) rt_handle: tokio::runtime::Handle,
    pub(super) resources: HashMap<ResourceId, Box<dyn StoredResource>>,
    pub(super) unstarted_tasks: Vec<TaskRepr>,
    pub(super) join_handles: Vec<Fuse<JoinHandle<anyhow::Result<()>>>>,
}

impl fmt::Debug for PreRun {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let resources = self.resources.keys().collect::<Vec<_>>();
        let tasks = self
            .unstarted_tasks
            .iter()
            .map(|t| &t.name)
            .collect::<Vec<_>>();
        f.debug_struct("PreRun")
            .field("resources", &resources)
            .field("tasks", &tasks)
            .finish_non_exhaustive()
    }
}

impl PreRun {
    /// Launches a task specified by its name.
    /// Returns an error if there is no such task or if it's already running.
    pub fn launch_task(&mut self, name: &str) -> anyhow::Result<()> {
        let task = self
            .unstarted_tasks
            .iter()
            .position(|t| t.name == name)
            .ok_or_else(|| anyhow::format_err!("Task {} not found", name))?;
        let task_future = self.unstarted_tasks[task]
            .task
            .take()
            .ok_or_else(|| anyhow::format_err!("Task {} is already running", name))?;
        self.join_handles
            .push(self.rt_handle.spawn(task_future).fuse());
        Ok(())
    }

    /// Attempts to retrieve a resource.
    /// Returns `None` if the resource is not found.
    pub fn get_resource<T: Resource + Clone>(&self) -> Option<T> {
        let name = T::resource_id();
        // Check whether the resource is already available.
        if let Some(resource) = self.resources.get(&name) {
            return Some(downcast_clone(resource));
        }
        None
    }
}
