use std::time::Instant;

use cliclack::{spinner, ProgressBar};

use crate::config::global_config;

/// Spinner is a helper struct to show a spinner while some operation is running.
pub struct Spinner {
    msg: String,
    pb: ProgressBar,
    time: Instant,
}

impl Spinner {
    /// Create a new spinner with a message.
    pub fn new(msg: &str) -> Self {
        let pb = spinner();
        pb.start(msg);
        if global_config().verbose {
            pb.stop(msg);
        }
        Spinner {
            msg: msg.to_owned(),
            pb,
            time: Instant::now(),
        }
    }

    /// Manually finish the spinner.
    pub fn finish(self) {
        self.pb.stop(format!(
            "{} done in {} secs",
            self.msg,
            self.time.elapsed().as_secs_f64()
        ));
    }

    /// Interrupt the spinner with a failed message.
    pub fn fail(self) {
        self.pb.error(format!(
            "{} failed in {} secs",
            self.msg,
            self.time.elapsed().as_secs_f64()
        ));
    }

    /// Freeze the spinner with current message.
    pub fn freeze(self) {
        self.pb.stop(self.msg);
    }
}
