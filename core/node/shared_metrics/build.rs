//! Build script for the external node binary.

use std::{
    env, fs,
    io::{self, Write},
    path::Path,
    process::Command,
};

use rustc_version::{Channel, LlvmVersion};

fn print_rust_meta(out: &mut impl Write, meta: &rustc_version::VersionMeta) -> io::Result<()> {
    writeln!(
        out,
        "pub const RUSTC_METADATA: RustcMetadata = RustcMetadata {{ \
            version: {semver:?}, \
            commit_hash: {commit_hash:?}, \
            commit_date: {commit_date:?}, \
            channel: {channel:?}, \
            host: {host:?}, \
            llvm: {llvm:?}, \
            git_branch: {git_branch:?}, \
            git_revision: {git_revision:?} \
        }};",
        semver = meta.semver.to_string(),
        commit_hash = meta.commit_hash,
        commit_date = meta.commit_date,
        channel = match meta.channel {
            Channel::Dev => "dev",
            Channel::Beta => "beta",
            Channel::Nightly => "nightly",
            Channel::Stable => "stable",
        },
        host = meta.host,
        llvm = meta.llvm_version.as_ref().map(LlvmVersion::to_string),
        git_branch = git_branch(),
        git_revision = git_revision()
    )
}

/// Outputs the current git branch as a string literal.
pub fn git_branch() -> String {
    run_cmd("git", &["rev-parse", "--abbrev-ref", "HEAD"])
}

/// Outputs the current git commit hash as a string literal.
pub fn git_revision() -> String {
    run_cmd("git", &["rev-parse", "--short", "HEAD"])
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

fn main() {
    let out_dir = env::var("OUT_DIR").expect("`OUT_DIR` env var not set for build script");
    let rustc_meta = rustc_version::version_meta().expect("Failed obtaining rustc metadata");

    let metadata_module_path = Path::new(&out_dir).join("metadata_values.rs");
    let metadata_module =
        fs::File::create(metadata_module_path).expect("cannot create metadata module");
    let mut metadata_module = io::BufWriter::new(metadata_module);

    print_rust_meta(&mut metadata_module, &rustc_meta).expect("failed printing rustc metadata");
}
