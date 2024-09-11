use std::{
    ffi::OsStr,
    fmt::{Display, Formatter},
    io,
    process::{Command, Output, Stdio},
    string::FromUtf8Error,
};

use console::style;

use crate::{
    config::global_config,
    logger::{self},
};

/// A wrapper around [`xshell::Cmd`] that allows for improved error handling,
/// and verbose logging.
#[derive(Debug)]
pub struct Cmd<'a> {
    inner: xshell::Cmd<'a>,
    force_run: bool,
    // For resume functionality we must pipe the output, otherwise it only shows less information
    piped_std_err: bool,
}

#[derive(thiserror::Error, Debug)]
pub struct CmdError {
    pub stderr: Option<String>,
    pub source: anyhow::Error,
}

impl Display for CmdError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut data = format!("{}", &self.source);
        if let Some(stderr) = &self.stderr {
            data = format!("{data}\n{stderr}");
        }
        write!(f, "{}", data)
    }
}

impl From<xshell::Error> for CmdError {
    fn from(value: xshell::Error) -> Self {
        Self {
            stderr: None,
            source: value.into(),
        }
    }
}

impl From<io::Error> for CmdError {
    fn from(value: io::Error) -> Self {
        Self {
            stderr: None,
            source: value.into(),
        }
    }
}

impl From<FromUtf8Error> for CmdError {
    fn from(value: FromUtf8Error) -> Self {
        Self {
            stderr: None,
            source: value.into(),
        }
    }
}

pub type CmdResult<T> = Result<T, CmdError>;

impl<'a> Cmd<'a> {
    /// Create a new `Cmd` instance.
    pub fn new(cmd: xshell::Cmd<'a>) -> Self {
        Self {
            inner: cmd,
            force_run: false,
            piped_std_err: false,
        }
    }

    /// Run the command printing the output to the console.
    pub fn with_force_run(mut self) -> Self {
        self.force_run = true;
        self
    }

    pub fn with_piped_std_err(mut self) -> Self {
        self.piped_std_err = true;
        self
    }

    /// Set env variables for the command.
    pub fn env<K: AsRef<OsStr>, V: AsRef<OsStr>>(mut self, key: K, value: V) -> Self {
        self.inner = self.inner.env(key, value);
        self
    }

    /// Run the command without capturing its output.
    pub fn run(mut self) -> CmdResult<()> {
        let command_txt = self.inner.to_string();
        let output = if global_config().verbose || self.force_run {
            logger::debug(format!("Running: {}", self.inner));
            logger::new_empty_line();
            let output = run_low_level_process_command(self.inner.into(), self.piped_std_err)?;
            if let Ok(data) = String::from_utf8(output.stderr.clone()) {
                if !data.is_empty() {
                    logger::info(data)
                }
            }
            output
        } else {
            // Command will be logged manually.
            self.inner.set_quiet(true);
            // Error will be handled manually.
            self.inner.set_ignore_status(true);
            self.inner.output()?
        };

        check_output_status(&command_txt, &output)?;
        if global_config().verbose {
            logger::debug(format!("Command completed: {}", command_txt));
        }

        Ok(())
    }

    /// Run the command and return its output.
    pub fn run_with_output(&mut self) -> CmdResult<std::process::Output> {
        if global_config().verbose || self.force_run {
            logger::debug(format!("Running: {}", self.inner));
            logger::new_empty_line();
        }

        self.inner.set_ignore_status(true);
        let output = self.inner.output()?;

        if global_config().verbose || self.force_run {
            logger::raw(log_output(&output));
            logger::new_empty_line();
            logger::new_line();
        }

        Ok(output)
    }
}

fn check_output_status(command_text: &str, output: &std::process::Output) -> CmdResult<()> {
    if !output.status.success() {
        logger::new_line();
        logger::error_note(
            &format!("Command failed to run: {}", command_text),
            &log_output(output),
        );
        return Err(CmdError {
            stderr: Some(String::from_utf8(output.stderr.clone())?),
            source: anyhow::anyhow!("Command failed to run: {}", command_text),
        });
    }

    Ok(())
}

fn run_low_level_process_command(mut command: Command, piped_std_err: bool) -> io::Result<Output> {
    command.stdout(Stdio::inherit());
    if piped_std_err {
        command.stderr(Stdio::piped());
    } else {
        command.stderr(Stdio::inherit());
    }
    let child = command.spawn()?;
    child.wait_with_output()
}

fn log_output(output: &std::process::Output) -> String {
    let (status, stdout, stderr) = get_indented_output(output, 4, 120);
    log_output_int(status, Some(stdout), Some(stderr))
}

fn log_output_int(status: String, stdout: Option<String>, stderr: Option<String>) -> String {
    let status_header = style("  Status:").bold();
    let stdout = if let Some(stdout) = stdout {
        let stdout_header = style("  Stdout:").bold();
        format!("{stdout_header}\n{stdout}\n")
    } else {
        String::new()
    };

    let stderr = if let Some(stderr) = stderr {
        let stderr_header = style("  Stderr:").bold();
        format!("{stderr_header}\n{stderr}\n")
    } else {
        String::new()
    };

    format!("{status_header}\n{status}\n{stdout}\n{stderr}")
}

// Indent output and wrap text.
fn get_indented_output(
    output: &std::process::Output,
    indentation: usize,
    wrap: usize,
) -> (String, String, String) {
    let status = output.status.to_string();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let indent = |s: &str| {
        s.lines()
            .map(|l| format!("{:indent$}{}", "", l, indent = indentation))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let wrap_text_to_len = |s: &str| {
        let mut result = String::new();

        for original_line in s.split('\n') {
            if original_line.trim().is_empty() {
                result.push('\n');
                continue;
            }

            let mut line = String::new();
            for word in original_line.split_whitespace() {
                if line.len() + word.len() + 1 > wrap {
                    result.push_str(&line);
                    result.push('\n');
                    line.clear();
                }
                if !line.is_empty() {
                    line.push(' ');
                }
                line.push_str(word);
            }
            result.push_str(&line);
            result.push('\n');
        }

        result
    };

    (
        indent(&wrap_text_to_len(&status)),
        indent(&wrap_text_to_len(&stdout)),
        indent(&wrap_text_to_len(&stderr)),
    )
}
