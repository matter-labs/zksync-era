use std::sync::{Arc, Weak};

/// FIXME: document
#[derive(Debug)]
pub struct StopGuard {
    _inner: Arc<()>,
}

impl StopGuard {
    pub fn new() -> (Self, StopToken) {
        let arc = Arc::new(());
        let token = StopToken(Arc::downgrade(&arc));
        (Self { _inner: arc }, token)
    }
}

#[derive(Debug, Clone)]
pub struct StopToken(Weak<()>);

impl StopToken {
    pub fn should_stop(&self) -> bool {
        self.0.strong_count() == 0
    }
}
