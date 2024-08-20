use crate::Halt;

#[derive(Debug, Clone, PartialEq)]
pub enum TracerExecutionStopReason {
    Finish,
    Abort(Halt),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TracerExecutionStatus {
    Continue,
    Stop(TracerExecutionStopReason),
}

impl TracerExecutionStatus {
    /// Chose the stricter ExecutionStatus
    /// If both statuses are Continue, then the result is Continue
    /// If one of the statuses is Abort, then the result is Abort
    /// If one of the statuses is Finish, then the result is Finish
    pub fn stricter(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Continue, Self::Continue) => Self::Continue,
            (Self::Stop(TracerExecutionStopReason::Abort(reason)), _)
            | (_, Self::Stop(TracerExecutionStopReason::Abort(reason))) => {
                Self::Stop(TracerExecutionStopReason::Abort(reason.clone()))
            }
            (Self::Stop(TracerExecutionStopReason::Finish), _)
            | (_, Self::Stop(TracerExecutionStopReason::Finish)) => {
                Self::Stop(TracerExecutionStopReason::Finish)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmExecutionStopReason {
    VmFinished,
    TracerRequestedStop(TracerExecutionStopReason),
}
