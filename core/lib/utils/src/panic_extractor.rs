use tokio::task::JoinError;

pub fn try_extract_panic_message(err: JoinError) -> String {
    if err.is_panic() {
        let panic = err.into_panic();
        if let Some(panic_string) = panic.downcast_ref::<&'static str>() {
            panic_string.to_string()
        } else if let Some(panic_string) = panic.downcast_ref::<String>() {
            panic_string.to_string()
        } else {
            "Unknown panic".to_string()
        }
    } else {
        "Cancelled task".to_string()
    }
}
