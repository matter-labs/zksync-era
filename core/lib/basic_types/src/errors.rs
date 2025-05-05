use std::fmt;

#[derive(Debug)]
pub enum OrStopped<E = anyhow::Error> {
    Internal(E),
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

pub trait StopContext<T>: Sized {
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
