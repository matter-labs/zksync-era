use pin_project_lite::pin_project;
use std::{future::Future, pin::Pin, task};

pin_project! {
    /// Implements a future with the name tag attached.
    #[derive(Debug)]
    pub struct NamedFuture<F> {
        #[pin]
        inner: F,
        name: String,
    }
}

impl<T, F> NamedFuture<F>
where
    F: Future<Output = T>,
{
    /// Creates a new future with the name tag attached.
    pub fn new(inner: F, name: String) -> Self {
        Self { inner, name }
    }

    pub fn name(&self) -> &str {
        &self.name
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
