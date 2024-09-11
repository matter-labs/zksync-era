use std::{fmt, future::Future};

use futures::{future::BoxFuture, FutureExt};

use crate::{IntoContext, TaskId};

/// A named future that will be invoked after all the tasks are stopped.
/// The future is expected to perform a cleanup or a shutdown of the service.
///
/// All the shutdown hooks will be executed sequentially, so they may assume that
/// no other tasks are running at the moment of execution on the same node. However,
/// an unique access to the database is not guaranteed, since the node may run in a
/// distributed mode, so this should not be used for potentially destructive actions.
pub struct ShutdownHook {
    pub(crate) id: TaskId,
    pub(crate) future: BoxFuture<'static, anyhow::Result<()>>,
}

impl fmt::Debug for ShutdownHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShutdownHook")
            .field("name", &self.id)
            .finish()
    }
}

impl ShutdownHook {
    pub fn new(
        name: &'static str,
        hook: impl Future<Output = anyhow::Result<()>> + Send + 'static,
    ) -> Self {
        Self {
            id: name.into(),
            future: hook.boxed(),
        }
    }
}

impl IntoContext for ShutdownHook {
    fn into_context(
        self,
        context: &mut super::ServiceContext<'_>,
    ) -> Result<(), crate::WiringError> {
        context.add_shutdown_hook(self);
        Ok(())
    }
}
