use std::{borrow::Cow, fmt};

/// Task kind.
/// See [`Task`](super::Task) documentation for more details.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum TaskKind {
    Task,
    OneshotTask,
    UnconstrainedTask,
    UnconstrainedOneshotTask,
    Precondition,
}

impl TaskKind {
    pub(crate) fn is_oneshot(self) -> bool {
        matches!(
            self,
            TaskKind::OneshotTask | TaskKind::UnconstrainedOneshotTask | TaskKind::Precondition
        )
    }
}

/// A unique human-readable identifier of a task.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(pub(crate) Cow<'static, str>);

impl fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl From<&'static str> for TaskId {
    fn from(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }
}

impl From<String> for TaskId {
    fn from(value: String) -> Self {
        Self(Cow::Owned(value))
    }
}
