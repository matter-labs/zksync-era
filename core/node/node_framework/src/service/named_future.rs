use std::{fmt, future::Future, pin::Pin, task};

use pin_project_lite::pin_project;

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
    F: Future,
{
    /// Creates a new future with the name tag attached.
    pub fn new(inner: F, name: TaskId) -> Self {
        Self { inner, name }
    }

    pub fn id(&self) -> TaskId {
        self.name.clone()
    }

    pub fn into_inner(self) -> F {
        self.inner
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
