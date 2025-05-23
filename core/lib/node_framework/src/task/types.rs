use std::{
    fmt::{Display, Formatter},
    ops::Deref,
};

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
pub struct TaskId(String);

impl TaskId {
    pub fn new(value: String) -> Self {
        TaskId(value)
    }
}

impl Display for TaskId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for TaskId {
    fn from(value: &str) -> Self {
        TaskId(value.to_owned())
    }
}

impl From<String> for TaskId {
    fn from(value: String) -> Self {
        TaskId(value)
    }
}

impl Deref for TaskId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
