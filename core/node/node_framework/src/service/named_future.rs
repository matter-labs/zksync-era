use std::{fmt, future::Future, pin::Pin, task};

use futures::future::{Fuse, FutureExt};
use pin_project_lite::pin_project;
use tokio::task::JoinHandle;

use crate::task::TaskId;

pin_project! {
    /// Implements a future with the name tag attached.
    pub struct NamedFuture<F> {
        #[pin]
        inner: F,
        name: TaskId,
    }
}

impl<F> NamedFuture<F>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    /// Creates a new future with the name tag attached.
    pub fn new(inner: F, name: TaskId) -> Self {
        Self { inner, name }
    }

    /// Returns the ID of the task attached to the future.
    pub fn id(&self) -> TaskId {
        self.name.clone()
    }

    /// Fuses the wrapped future.
    pub fn fuse(self) -> NamedFuture<Fuse<F>> {
        NamedFuture {
            name: self.name,
            inner: self.inner.fuse(),
        }
    }

    /// Spawns the wrapped future on the provided runtime handle.
    /// Returns a named wrapper over the join handle.
    pub fn spawn(self, handle: &tokio::runtime::Handle) -> NamedFuture<JoinHandle<F::Output>> {
        NamedFuture {
            name: self.name,
            inner: handle.spawn(self.inner),
        }
    }
}

impl<F> Future for NamedFuture<F>
where
    F: Future,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        tracing::info_span!("NamedFuture", name = %self.name)
            .in_scope(|| self.project().inner.poll(cx))
    }
}

impl<F> fmt::Debug for NamedFuture<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NamedFuture")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
