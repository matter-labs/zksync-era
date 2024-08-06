#[derive(Debug)]
pub(crate) enum Hook {
    AccountValidationEntered,
    AccountValidationExited,
    ValidationStepEnded,
    TxHasEnded,
    DebugLog,
    DebugReturnData,
    AskOperatorForRefund,
    NotifyAboutRefund,
    PostResult,
    FinalBatchInfo,
}

impl Hook {
    /// # Panics
    /// Panics if the number does not correspond to any hook.
    pub fn from_u32(hook: u32) -> Self {
        match hook {
            0 => Hook::AccountValidationEntered,
            2 => Hook::AccountValidationExited,
            3 => Hook::ValidationStepEnded,
            4 => Hook::TxHasEnded,
            5 => Hook::DebugLog,
            6 => Hook::DebugReturnData,
            8 => Hook::AskOperatorForRefund,
            9 => Hook::NotifyAboutRefund,
            10 => Hook::PostResult,
            11 => Hook::FinalBatchInfo,
            _ => panic!("Unknown hook {}", hook),
        }
    }
}
