use std::sync::{Arc, Weak};

/// Guard allowing to stop blocking operations when dropped.
///
/// # Usage
///
/// 1. Instantiate a guard and the paired [`StopToken`] via [`Self::new()`] in a future.
/// 2. Leave the guard as-is inside the future.
/// 3. Send the token to a blocking Tokio task (i.e., a `tokio::task::spawn_blocking()` closure).
/// 4. Check the token state periodically via [`StopToken::should_stop()`] in the blocking task; stop the task if it is `true`.
///
/// # Why this works
///
/// If a future is dropped (e.g., by timing it out, selecting between it and another future, etc.),
/// blocking task(s) spawned inside the future will continue executing indefinitely because Rust / Tokio
/// don't provide means to stop threads (which means do exist in some OSes, but are notoriously unsafe to use).
/// `StopGuard` provides an app-level solution to this concern. It is a thin non-cloneable wrapper around an [`Arc`],
/// and `StopToken` is a (cloneable) [`Weak`] reference to it. `StopToken::should_stop()` checks whether the strong count
/// for the `Arc` is zero.
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

/// Cloneable token corresponding to a [`StopGuard`]. Can be used to check whether the guard is dropped.
#[derive(Debug, Clone)]
pub struct StopToken(Weak<()>);

impl StopToken {
    /// Returns `true` if the corresponding `StopGuard` has been dropped. If the guard is placed inside a future,
    /// this means that the future has been dropped / cancelled (via a timeout, selecting between it and another future etc.).
    pub fn should_stop(&self) -> bool {
        self.0.strong_count() == 0
    }
}
