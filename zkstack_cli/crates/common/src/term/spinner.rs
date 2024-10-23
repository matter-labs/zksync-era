use std::{fmt::Display, io::IsTerminal, time::Instant};

use cliclack::{spinner, ProgressBar};

use crate::{config::global_config, logger};

/// Spinner is a helper struct to show a spinner while some operation is running.
pub struct Spinner {
    msg: String,
    output: SpinnerOutput,
    time: Instant,
}

impl Spinner {
    /// Create a new spinner with a message.
    pub fn new(msg: &str) -> Self {
        let output = if std::io::stdout().is_terminal() {
            let pb = spinner();
            pb.start(msg);
            if global_config().verbose {
                pb.stop(msg);
            }
            SpinnerOutput::Progress(pb)
        } else {
            logger::info(msg);
            SpinnerOutput::Plain()
        };
        Spinner {
            msg: msg.to_owned(),
            output,
            time: Instant::now(),
        }
    }

    /// Manually finish the spinner.
    pub fn finish(self) {
        self.output.stop(format!(
            "{} done in {} secs",
            self.msg,
            self.time.elapsed().as_secs_f64()
        ));
    }

    /// Interrupt the spinner with a failed message.
    pub fn fail(self) {
        self.output.error(format!(
            "{} failed in {} secs",
            self.msg,
            self.time.elapsed().as_secs_f64()
        ));
    }

    /// Freeze the spinner with current message.
    pub fn freeze(self) {
        self.output.stop(self.msg);
    }
}

/// An abstraction that makes interactive progress bar optional in environments where virtual
/// terminal is not available.
///
/// Uses plain `logger::{info,error}` as the fallback.
///
/// See https://github.com/console-rs/indicatif/issues/530 for more details.
enum SpinnerOutput {
    Progress(ProgressBar),
    Plain(),
}

impl SpinnerOutput {
    fn error(&self, msg: impl Display) {
        match self {
            SpinnerOutput::Progress(pb) => pb.error(msg),
            SpinnerOutput::Plain() => logger::error(msg),
        }
    }

    fn stop(self, msg: impl Display) {
        match self {
            SpinnerOutput::Progress(pb) => pb.stop(msg),
            SpinnerOutput::Plain() => logger::info(msg),
        }
    }
}
