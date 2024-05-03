//! Build script for the external node binary.

use std::{
    env, fs,
    io::{self, Write},
    path::Path,
};

use rustc_version::{Channel, LlvmVersion};

fn print_rust_meta(out: &mut impl Write, meta: &rustc_version::VersionMeta) -> io::Result<()> {
    writeln!(
        out,
        "pub(crate) const RUSTC_METADATA: RustcMetadata = RustcMetadata {{ \
            version: {semver:?}, \
            commit_hash: {commit_hash:?}, \
            commit_date: {commit_date:?}, \
            channel: {channel:?}, \
            host: {host:?}, \
            llvm: {llvm:?} \
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
    )
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
