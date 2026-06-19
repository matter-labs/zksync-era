//! Build-time fetch of the Airbender guest program and FRI verification key.
//!
//! The guest artifacts (`app.bin` / `app.text`) and the FRI verification key
//! (`fri_vk.bin`) are **not** committed to this repo. Instead they are pulled
//! from the GitHub release of `zksync_airbender_verifier` that matches the
//! version this crate pins, so the artifacts can never drift out of sync with
//! the verifier code we compile against.
//!
//! Flow:
//! 1. Read `cargo metadata` and find the `zksync_airbender_verifier` package.
//!    Its source spec encodes the git repository and the pinned ref; the
//!    release tag is taken from an explicit `?tag=` if present, otherwise
//!    derived from the package version (`v{version}`, the release-please tag
//!    convention used by the verifier repo).
//! 2. Download each artifact from
//!    `https://github.com/<owner>/<repo>/releases/download/<tag>/<asset>` into a
//!    local cache (`guest/dist/app` and `vks/`). Downloads are skipped when the
//!    file is already present for the same tag, so incremental builds don't
//!    re-fetch.
//! 3. Export the resolved paths as `AIRBENDER_GUEST_DIST_DIR` / `AIRBENDER_FRI_VK`
//!    so `main.rs` can bake them in as the compile-time default locations.
//!
//! Escape hatches (for offline / air-gapped / prebuilt-artifact builds): set
//! `AIRBENDER_GUEST_DIST_DIR` and/or `AIRBENDER_FRI_VK` in the build environment
//! to point at existing files; the corresponding download is then skipped and
//! the provided path is re-exported verbatim.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Package whose release we download artifacts from.
const VERIFIER_PACKAGE: &str = "zksync_airbender_verifier";

fn main() {
    // Re-run when the build script itself or the dependency pin changes.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-env-changed=AIRBENDER_GUEST_DIST_DIR");
    println!("cargo:rerun-if-env-changed=AIRBENDER_FRI_VK");

    let manifest_dir = PathBuf::from(env_var("CARGO_MANIFEST_DIR"));

    // Resolve where the verifier release lives from cargo metadata.
    let release = resolve_release();

    // Guest program: `guest/dist/app/{app.bin,app.text}`.
    let guest_dir = match std::env::var_os("AIRBENDER_GUEST_DIST_DIR") {
        // Operator override: trust the provided directory as-is.
        Some(dir) => PathBuf::from(dir),
        None => {
            let dir = manifest_dir.join("guest/dist/app");
            ensure_artifact(&release, "app.bin", &dir.join("app.bin"));
            ensure_artifact(&release, "app.text", &dir.join("app.text"));
            dir
        }
    };

    // FRI verification key: `vks/fri_vk.bin`.
    let fri_vk = match std::env::var_os("AIRBENDER_FRI_VK") {
        Some(path) => PathBuf::from(path),
        None => {
            let path = manifest_dir.join("vks/fri_vk.bin");
            ensure_artifact(&release, "fri_vk.bin", &path);
            path
        }
    };

    // Bake the resolved locations in as compile-time defaults for `main.rs`.
    println!(
        "cargo:rustc-env=AIRBENDER_GUEST_DIST_DIR={}",
        guest_dir.display()
    );
    println!("cargo:rustc-env=AIRBENDER_FRI_VK={}", fri_vk.display());
}

/// A resolved GitHub release to download artifacts from.
struct Release {
    /// Base repo URL, e.g. `https://github.com/matter-labs/eravm-airbender-verifier`.
    repo_url: String,
    /// Release tag, e.g. `v0.3.1`.
    tag: String,
}

impl Release {
    /// Download URL for a named release asset.
    fn asset_url(&self, asset: &str) -> String {
        format!("{}/releases/download/{}/{}", self.repo_url, self.tag, asset)
    }
}

/// Inspect `cargo metadata` to find the verifier package and derive the release
/// repo + tag it corresponds to.
fn resolve_release() -> Release {
    let metadata = run_cargo_metadata();
    let package = metadata["packages"]
        .as_array()
        .expect("cargo metadata: `packages` is not an array")
        .iter()
        .find(|p| p["name"].as_str() == Some(VERIFIER_PACKAGE))
        .unwrap_or_else(|| {
            panic!("cargo metadata: package `{VERIFIER_PACKAGE}` not found in dependency graph")
        });

    let source = package["source"]
        .as_str()
        .unwrap_or_else(|| panic!("`{VERIFIER_PACKAGE}` has no `source` (is it a path dependency?)"));
    let version = package["version"]
        .as_str()
        .unwrap_or_else(|| panic!("`{VERIFIER_PACKAGE}` has no `version`"));

    let repo_url = parse_git_repo(source);
    // Prefer an explicit `?tag=`; otherwise fall back to the release-please tag
    // convention (`v{version}`). Either way the artifacts track the exact
    // verifier version we compile against.
    let tag = tag_from_source(source).unwrap_or_else(|| format!("v{version}"));

    Release { repo_url, tag }
}

/// Run `cargo metadata` and parse it as JSON. Uses the `CARGO` cargo sets for
/// build scripts so the right toolchain is invoked.
fn run_cargo_metadata() -> serde_json::Value {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version=1"])
        .current_dir(env_var("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run `cargo metadata`");
    assert!(
        output.status.success(),
        "`cargo metadata` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("failed to parse `cargo metadata` JSON")
}

/// Extract the base repo URL from a cargo git source spec such as
/// `git+https://github.com/matter-labs/eravm-airbender-verifier?rev=abc#abc`.
fn parse_git_repo(source: &str) -> String {
    let url = source
        .strip_prefix("git+")
        .unwrap_or_else(|| panic!("`{VERIFIER_PACKAGE}` source is not a git dependency: {source}"));
    // Drop the `?query` and `#fragment` parts, then a trailing `.git`.
    let url = url.split(['?', '#']).next().unwrap_or(url);
    url.strip_suffix(".git")
        .unwrap_or(url)
        .trim_end_matches('/')
        .to_owned()
}

/// Pull `tag` out of the source spec's query string, if pinned by tag.
fn tag_from_source(source: &str) -> Option<String> {
    let query = source.split('?').nth(1)?;
    let query = query.split('#').next().unwrap_or(query);
    query.split('&').find_map(|kv| {
        kv.strip_prefix("tag=")
            .map(|t| t.to_owned())
    })
}

/// Download `asset` to `dest` unless a usable copy is already cached there.
///
/// A sibling `<dest>.tag` marker records which release a downloaded file came
/// from. We skip the download when the file exists *and* either has no marker
/// (a pre-existing / operator-placed copy we trust) or a marker matching the
/// current tag. We only re-download when a marker is present but records a
/// *different* tag — i.e. the verifier pin moved since we last fetched.
fn ensure_artifact(release: &Release, asset: &str, dest: &Path) {
    let marker = dest.with_extension(format!(
        "{}.tag",
        dest.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    if dest.exists() {
        match read_marker(&marker) {
            None => return,
            Some(tag) if tag == release.tag => return,
            Some(_) => {} // stale: pin moved, fall through to re-download.
        }
    }

    let url = release.asset_url(asset);
    println!("cargo:warning=downloading {asset} from {url}");
    let bytes = download(&url);

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .unwrap_or_else(|e| panic!("failed to create {}: {e}", parent.display()));
    }
    std::fs::write(dest, &bytes)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", dest.display()));
    std::fs::write(&marker, &release.tag)
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", marker.display()));
}

fn read_marker(marker: &Path) -> Option<String> {
    std::fs::read_to_string(marker)
        .ok()
        .map(|s| s.trim().to_owned())
}

/// Blocking GET that fails the build on any non-success status.
fn download(url: &str) -> Vec<u8> {
    let response = reqwest::blocking::get(url)
        .unwrap_or_else(|e| panic!("request to {url} failed: {e}"));
    let status = response.status();
    assert!(
        status.is_success(),
        "downloading {url} returned HTTP {status}"
    );
    response
        .bytes()
        .unwrap_or_else(|e| panic!("reading body of {url} failed: {e}"))
        .to_vec()
}

fn env_var(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| panic!("missing build env var `{key}`"))
}
