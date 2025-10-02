const GIT_VERSION: &str = zkstack_cli_git_version_macro::build_git_revision!();
const GIT_BRANCH: &str = zkstack_cli_git_version_macro::build_git_branch!();
const GIT_SUBMODULES: &[(&str, &str)] = zkstack_cli_git_version_macro::build_git_submodules!();
const BUILD_TIMESTAMP: &str = zkstack_cli_git_version_macro::build_timestamp!();

/// Returns a multi-line version message that includes:
/// - provided crate version
/// - git revision
/// - git branch
/// - git submodules
/// - build timestamp
pub fn version_message(crate_version: &str) -> String {
    let mut version = format!("v{}-{}\n", crate_version, GIT_VERSION);
    version.push_str(&format!("Branch: {}\n", GIT_BRANCH));
    #[allow(clippy::const_is_empty)] // Proc-macro generated.
    if !GIT_SUBMODULES.is_empty() {
        version.push_str("Submodules:\n");
        for (name, rev) in GIT_SUBMODULES {
            version.push_str(&format!("  - {}: {}\n", name, rev));
        }
    }
    version.push_str(&format!("Build timestamp: {}\n", BUILD_TIMESTAMP));
    version
}
