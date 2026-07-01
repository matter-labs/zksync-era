//! Downloads the guest program + verification keys from the GitHub release of
//! `zksync_airbender_verifier` matching the version this crate pins, so they
//! never drift from the verifier code we compile against and need not be
//! committed. The resolved paths are exported as `AIRBENDER_GUEST_DIST_DIR` /
//! `AIRBENDER_FRI_VK` / `AIRBENDER_SNARK_VK` for `main.rs` to bake in as
//! defaults. Set any of those env vars at build time to use prebuilt files and
//! skip the corresponding download (offline / air-gapped builds).

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
    println!("cargo:rerun-if-env-changed=AIRBENDER_SNARK_VK");

    let manifest_dir = PathBuf::from(env_var("CARGO_MANIFEST_DIR"));
    let release = resolve_release();

    // Guest program: `guest/dist/app/{app.bin,app.text}`.
    let guest_dir = match std::env::var_os("AIRBENDER_GUEST_DIST_DIR") {
        Some(dir) => PathBuf::from(dir), // override: trust as-is.
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

    // SNARK wrapper verification key: `vks/snark_vk.json`.
    let snark_vk = match std::env::var_os("AIRBENDER_SNARK_VK") {
        Some(path) => PathBuf::from(path),
        None => {
            let path = manifest_dir.join("vks/snark_vk.json");
            ensure_artifact(&release, "snark_vk.json", &path);
            path
        }
    };

    // Export the resolved locations as `main.rs`'s compile-time defaults.
    println!(
        "cargo:rustc-env=AIRBENDER_GUEST_DIST_DIR={}",
        guest_dir.display()
    );
    println!("cargo:rustc-env=AIRBENDER_FRI_VK={}", fri_vk.display());
    println!("cargo:rustc-env=AIRBENDER_SNARK_VK={}", snark_vk.display());
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

/// Find the verifier package in `cargo metadata` and derive its release.
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

    let source = package["source"].as_str().unwrap_or_else(|| {
        panic!("`{VERIFIER_PACKAGE}` has no `source` (is it a path dependency?)")
    });
    let version = package["version"]
        .as_str()
        .unwrap_or_else(|| panic!("`{VERIFIER_PACKAGE}` has no `version`"));

    let repo_url = parse_git_repo(source);
    // An explicit `?tag=`, else the release-please convention `v{version}`.
    let tag = tag_from_source(source).unwrap_or_else(|| format!("v{version}"));

    Release { repo_url, tag }
}

/// Run `cargo metadata` and parse it as JSON.
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

/// Base repo URL from a cargo git source spec like
/// `git+https://github.com/owner/repo?rev=abc#abc`.
fn parse_git_repo(source: &str) -> String {
    let url = source
        .strip_prefix("git+")
        .unwrap_or_else(|| panic!("`{VERIFIER_PACKAGE}` source is not a git dependency: {source}"));
    // Drop `?query` / `#fragment`, then a trailing `.git`.
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
    query
        .split('&')
        .find_map(|kv| kv.strip_prefix("tag=").map(|t| t.to_owned()))
}

/// Download `asset` to `dest` unless already cached. A sibling `<dest>.tag`
/// marker records the release a download came from: we re-fetch only when the
/// marker records a *different* tag (the pin moved). A file with no marker (a
/// pre-existing / operator-placed copy) is trusted as-is.
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
    let response =
        reqwest::blocking::get(url).unwrap_or_else(|e| panic!("request to {url} failed: {e}"));
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
