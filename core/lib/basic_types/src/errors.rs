use std::fmt;

/// Represents an error in a stoppable task: either an internal / fatal error, or a task getting stopped
/// after receiving a stop request.
///
/// The [`Error`](std::error::Error) trait is intentionally not implemented for this enum to force users
/// to treat the `Stopped` variant with care. Depending on the application, it may not be an error.
/// Use the [`try_stoppable!` macro](crate::try_stoppable) to handle this variant by early-returning `Ok(())`.
#[derive(Debug)]
pub enum OrStopped<E = anyhow::Error> {
    /// Internal error.
    Internal(E),
    /// Graceful stop after receiving the corresponding request.
    Stopped,
}

impl fmt::Display for OrStopped {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Internal(err) => write!(formatter, "{err:#}"),
            Self::Stopped => formatter.write_str("gracefully stopped"),
        }
    }
}

impl<E> From<E> for OrStopped<E> {
    fn from(err: E) -> Self {
        Self::Internal(err)
    }
}

impl<E> OrStopped<E> {
    /// Maps an error to the `Internal` variant.
    pub fn internal(err: impl Into<E>) -> Self {
        Self::Internal(err.into())
    }
}

/// Extension trait for `Result<_, OrStopped>` similar to [`anyhow::Context`].
pub trait StopContext<T>: Sized {
    /// Adds context to the internal error, if any.
    fn stop_context(self, context: &'static str) -> Result<T, OrStopped>;

    /// Unwraps the `Stopped` variant to the supplied `value`, and the `Internal` variant to the underlying `anyhow::Error`.
    fn unwrap_stopped(self, value: T) -> anyhow::Result<T>;
}

impl<T> StopContext<T> for Result<T, OrStopped> {
    fn stop_context(self, context: &'static str) -> Result<T, OrStopped> {
        self.map_err(|err| match err {
            OrStopped::Internal(err) => OrStopped::Internal(err.context(context)),
            OrStopped::Stopped => OrStopped::Stopped,
        })
    }

    fn unwrap_stopped(self, value: T) -> anyhow::Result<T> {
        self.or_else(|err| match err {
            OrStopped::Internal(err) => Err(err),
            OrStopped::Stopped => Ok(value),
        })
    }
}
