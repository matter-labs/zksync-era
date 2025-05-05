#[derive(Debug)]
pub enum OrStopped<E = anyhow::Error> {
    Internal(E),
    Stopped,
}

impl<E> From<E> for OrStopped<E> {
    fn from(err: E) -> Self {
        Self::Internal(err)
    }
}
