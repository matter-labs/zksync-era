use std::fmt;

/// Represents an error in a stoppable task: either an internal / fatal error, or a task getting stopped
/// after receiving a stop signal.
///
/// The [`Error`](std::error::Error) trait is intentionally not implemented for this enum to force users
/// to treat the `Stopped` variant with care. Depending on the application, it may not be an error.
/// Use the [`try_stoppable!` macro](crate::try_stoppable) to handle this variant by early-returning `Ok(())`.
#[derive(Debug)]
pub enum OrStopped<E = anyhow::Error> {
    /// Internal error.
    Internal(E),
    /// Stop after receiving a signal.
    Stopped,
}

impl fmt::Display for OrStopped {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Internal(err) => write!(formatter, "{err:#}"),
            Self::Stopped => formatter.write_str("stopped by signal"),
        }
    }
}

impl<E> From<E> for OrStopped<E> {
    fn from(err: E) -> Self {
        Self::Internal(err)
    }
}

/// Extension trait for `Result<_, OrStopped>` similar to [`anyhow::Context`].
pub trait StopContext<T>: Sized {
    /// Adds context to the internal error, if any.
    fn stop_context(self, context: &'static str) -> Result<T, OrStopped>;
}

impl<T> StopContext<T> for Result<T, OrStopped> {
    fn stop_context(self, context: &'static str) -> Result<T, OrStopped> {
        self.map_err(|err| match err {
            OrStopped::Internal(err) => OrStopped::Internal(err.context(context)),
            OrStopped::Stopped => OrStopped::Stopped,
        })
    }
}
