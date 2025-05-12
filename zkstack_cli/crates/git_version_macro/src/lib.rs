extern crate proc_macro;
use std::{process::Command, str::FromStr};

use proc_macro::TokenStream;

/// Outputs the current date and time as a string literal.
/// Can be used to include the build timestamp in the binary.
#[proc_macro]
pub fn build_timestamp(_item: TokenStream) -> TokenStream {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    encode_as_str(&now)
}

/// Outputs the current git branch as a string literal.
#[proc_macro]
pub fn build_git_branch(_item: TokenStream) -> TokenStream {
    let out = run_cmd("git", &["rev-parse", "--abbrev-ref", "HEAD"]);
    encode_as_str(&out)
}

/// Outputs the current git commit hash as a string literal.
#[proc_macro]
pub fn build_git_revision(_item: TokenStream) -> TokenStream {
    let out = run_cmd("git", &["rev-parse", "--short", "HEAD"]);
    encode_as_str(&out)
}

/// Creates a slice of `&[(&str, &str)]` tuples that correspond to
/// the submodule name -> revision.
/// Results in an empty list if there are no submodules or if
/// the command fails.
#[proc_macro]
pub fn build_git_submodules(_item: TokenStream) -> TokenStream {
    let Some(out) = run_cmd_opt("git", &["submodule", "status"]) else {
        return TokenStream::from_str("&[]").unwrap();
    };
    let submodules = out
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            // Index 0 is commit hash, index 1 is the path to the folder, and there
            // may be some metainformation after that.
            if parts.len() >= 2 {
                let folder_name = parts[1].split('/').next_back().unwrap_or(parts[1]);
                Some((folder_name, parts[0]))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let submodules = submodules
        .iter()
        .map(|(name, rev)| format!("(\"{}\", \"{}\")", name, rev))
        .collect::<Vec<_>>()
        .join(", ");
    TokenStream::from_str(format!("&[{}]", submodules).as_str())
        .unwrap_or_else(|_| panic!("Unable to encode submodules: {}", submodules))
}

/// Tries to run the command, only returns `Some` if the command
/// succeeded and the output was valid utf8.
fn run_cmd(cmd: &str, args: &[&str]) -> String {
    run_cmd_opt(cmd, args).unwrap_or("unknown".to_string())
}

fn run_cmd_opt(cmd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(cmd).args(args).output().ok()?;
    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| s.trim().to_string())
    } else {
        None
    }
}

/// Encodes string as a literal.
fn encode_as_str(s: &str) -> TokenStream {
    TokenStream::from_str(format!("\"{}\"", s).as_str())
        .unwrap_or_else(|_| panic!("Unable to encode string: {}", s))
}
