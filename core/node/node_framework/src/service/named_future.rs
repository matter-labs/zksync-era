use pin_project_lite::pin_project;
use std::{future::Future, pin::Pin, task};

use crate::task::TaskId;

pin_project! {
    /// Implements a future with the name tag attached.
    #[derive(Debug)]
    pub struct NamedFuture<F> {
        #[pin]
        inner: F,
        name: TaskId,
    }
}

impl<T, F> NamedFuture<F>
where
    F: Future<Output = T>,
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

impl<T, F> Future for NamedFuture<F>
where
    F: Future<Output = T>,
{
    type Output = T;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        self.project().inner.poll(cx)
    }
}
