use std::{borrow::Cow, fmt, future::Future, pin::Pin, task};

use futures::{future::Fuse, FutureExt};
use pin_project_lite::pin_project;
use tokio::task::{JoinError, JoinHandle};
use zksync_instrument::alloc::AllocationGuard;

use crate::{metrics::METRICS, task::TaskId};

pin_project! {
    /// Implements a future with the name tag attached.
    pub(crate) struct NamedFuture<F> {
        #[pin]
        inner: F,
        name: &'static str,
    }
}

impl<F> NamedFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    /// Creates a new future with the name tag attached.
    pub(crate) fn new(inner: F, name: TaskId) -> Self {
        let name = match name.0 {
            Cow::Borrowed(name) => name,
            Cow::Owned(name) => {
                // Since we don't have that many tasks, we consider leaking their `name`s OK.
                name.leak()
            }
        };
        Self { inner, name }
    }

    /// Returns the ID of the task attached to the future.
    pub fn id(&self) -> TaskId {
        self.name.into()
    }

    /// Spawns the wrapped future on the provided runtime handle.
    /// Returns a named wrapper over the join handle.
    pub fn spawn(self, handle: &tokio::runtime::Handle) -> TaskFuture<F::Output> {
        TaskFuture {
            name: self.name,
            inner: handle.spawn(self).fuse(),
        }
    }
}

impl<F: Future> Future for NamedFuture<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        METRICS.poll_count[&self.name].inc();
        let _span_guard = tracing::info_span!("NamedFuture", name = %self.name).entered();
        let _alloc_guard = AllocationGuard::for_task(self.name);
        self.project().inner.poll(cx)
    }
}

impl<F> fmt::Debug for NamedFuture<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NamedFuture")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// Named future wrapper for a spawned Tokio task.
#[derive(Debug)]
pub(crate) struct TaskFuture<R = anyhow::Result<()>> {
    name: &'static str,
    inner: Fuse<JoinHandle<R>>,
}

impl<R> TaskFuture<R> {
    pub fn id(&self) -> TaskId {
        self.name.into()
    }
}

impl<R> Future for TaskFuture<R> {
    type Output = Result<R, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        self.inner.poll_unpin(cx)
    }
}
