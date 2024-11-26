//! Build script for the external node binary.

use std::{
    env, fs,
    io::{self, Write},
    path::Path,
    process::Command,
};

use rustc_version::{Channel, LlvmVersion};

fn print_binary_meta(out: &mut impl Write) -> io::Result<()> {
    let rustc_meta = rustc_version::version_meta().expect("Failed obtaining rustc metadata");

    writeln!(
        out,
        "pub const RUST_METADATA: RustMetadata = RustMetadata {{ \
            version: {semver:?}, \
            commit_hash: {commit_hash:?}, \
            commit_date: {commit_date:?}, \
            channel: {channel:?}, \
            host: {host:?}, \
            llvm: {llvm:?}, \
        }};

        pub const GIT_METADATA: GitMetadata = GitMetadata {{ \
            branch: {git_branch:?}, \
            revision: {git_revision:?} \
        }};",
        semver = rustc_meta.semver.to_string(),
        commit_hash = rustc_meta.commit_hash,
        commit_date = rustc_meta.commit_date,
        channel = match rustc_meta.channel {
            Channel::Dev => "dev",
            Channel::Beta => "beta",
            Channel::Nightly => "nightly",
            Channel::Stable => "stable",
        },
        host = rustc_meta.host,
        llvm = rustc_meta.llvm_version.as_ref().map(LlvmVersion::to_string),
        git_branch = git_branch(),
        git_revision = git_revision()
    )
}

/// Outputs the current git branch as a string literal.
pub fn git_branch() -> Option<String> {
    run_cmd_opt("git", &["rev-parse", "--abbrev-ref", "HEAD"])
}

/// Outputs the current git commit hash as a string literal.
pub fn git_revision() -> Option<String> {
    run_cmd_opt("git", &["rev-parse", "--short", "HEAD"])
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
    let metadata_module_path = Path::new(&out_dir).join("metadata_values.rs");
    let metadata_module =
        fs::File::create(metadata_module_path).expect("cannot create metadata module");
    let mut metadata_module = io::BufWriter::new(metadata_module);

    print_binary_meta(&mut metadata_module).expect("failed printing binary metadata");
}
