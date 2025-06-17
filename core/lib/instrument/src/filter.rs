//! Tools for filtering observed events.

use std::{
    cell::Cell,
    thread,
    time::{Duration, Instant},
};

/// Allows filtering events (e.g., for logging) so that they are reported no more frequently than with a configurable interval.
/// Created using the [`report_filter!`] macro.
///
/// # Implementation notes
///
/// Current implementation uses thread-local vars in order to not rely on mutexes or other cross-thread primitives.
/// I.e., it only really works if the number of threads accessing it is limited, e.g. if accessed in an async context
/// (in which case, the accessing threads are the Tokio runtime worker threads).
#[derive(Debug)]
pub struct ReportFilter {
    interval: Duration,
    last_timestamp: &'static thread::LocalKey<Cell<Option<Instant>>>,
}

impl ReportFilter {
    #[doc(hidden)] // Should only be used from the `report_filter!` macro.
    pub const fn new(
        interval: Duration,
        last_timestamp: &'static thread::LocalKey<Cell<Option<Instant>>>,
    ) -> Self {
        Self {
            interval,
            last_timestamp,
        }
    }

    /// Should be called sparingly, since it involves moderately heavy operations (getting the current time via a syscall).
    pub fn should_report(&self) -> bool {
        let timestamp = self.last_timestamp.get();
        let now = Instant::now();
        if timestamp.is_none_or(|ts| now - ts > self.interval) {
            self.last_timestamp.set(Some(now));
            true
        } else {
            false
        }
    }
}

/// Creates a new filter with the specified reporting interval *per thread*.
#[macro_export]
macro_rules! report_filter {
    ($interval:expr) => {{
        thread_local! {
            static LAST_TIMESTAMP: ::std::cell::Cell<::std::option::Option<::std::time::Instant>> =
                ::std::cell::Cell::new(::std::option::Option::None);
        }
        $crate::filter::ReportFilter::new($interval, &LAST_TIMESTAMP)
    }};
}

pub use report_filter;
