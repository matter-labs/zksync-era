use crate::config::global_config;
use cliclack::{spinner, ProgressBar};

/// Spinner is a helper struct to show a spinner while some operation is running.
pub struct Spinner {
    msg: String,
    pb: ProgressBar,
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
        }
    }

    /// Manually finish the spinner.
    pub fn finish(self) {
        self.pb.stop(format!("{} done", self.msg));
    }
}
