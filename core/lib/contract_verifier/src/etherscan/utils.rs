use super::solc_versions_fetcher::SolcVersionsFetcher;

/// Normalizes the zksolc version to the format expected by Etherscan.
pub(super) fn normalize_zksolc_version(version: Option<String>) -> Option<String> {
    match version {
        Some(version) => {
            // At the moment, there is one version that starts with the 'vm-' prefix: vm-1.5.0-a167aa3.
            // Since it's a prerelease version, it is not supported by Etherscan.
            // Try stripping the 'vm-' prefix and extracting the version number to verify using it.
            if version.starts_with("vm-") {
                version.split('-').nth(1).map(|ver| format!("v{}", ver))
            } else {
                Some(version.clone())
            }
        }
        None => None,
    }
}

/// Function that returns era solc version for zkVM compiler: version that comes after zkVM-X.Y.Z-.
fn get_zkvm_solc_era_version(solc_era_version: &str) -> String {
    match semver::Version::parse(solc_era_version) {
        // Since Etherscan doesn't support zkVM-X.Y.Z-1.0.0 versions, those are converted to zkVM-X.Y.Z-1.0.1.
        Ok(v) if v < semver::Version::new(1, 0, 1) => "1.0.1".to_string(),
        // Otherwise we return the version as is.
        Ok(_) | Err(_) => solc_era_version.to_string(),
    }
}

/// Normalizes the solc version to the format expected by Etherscan.
pub(super) fn normalize_solc_version(
    version: String,
    solc_versions_fetcher: &SolcVersionsFetcher,
) -> String {
    if version.contains("zkVM-") {
        let parts: Vec<&str> = version.split('-').collect();
        let zkvm_solc_version = parts.get(1).unwrap_or(&"");
        let zkvm_era_version = parts.get(2).unwrap_or(&"");
        return format!(
            "v{}-{}",
            zkvm_solc_version,
            // 1.0.0 - era version is transformed to 1.0.1
            // 1.0.1+ - era version remains the same
            get_zkvm_solc_era_version(zkvm_era_version)
        );
    }

    // Etherscan expects long solc versions to be passed in the request:
    // 0.8.28 -> 0.8.28+commit.7893614a etc.
    let version = solc_versions_fetcher
        .get_solc_long_version(&version)
        .unwrap_or(version);

    // Stored Solc versions and those returned by the SolcVersionsFetcher
    // do not have a leading 'v', but Etherscan requires it.
    if !version.starts_with("v") {
        return format!("v{}", version);
    }
    version
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_zksolc_version_with_vm_prefix() {
        let normalized_version = normalize_zksolc_version(Some("vm-1.5.0-a167aa3".to_string()));
        assert_eq!(normalized_version, Some("v1.5.0".to_string()));
    }

    #[test]
    fn test_normalize_zksolc_version_without_vm_prefix() {
        let normalized_version = normalize_zksolc_version(Some("v1.5.0".to_string()));
        assert_eq!(normalized_version, Some("v1.5.0".to_string()));
    }

    #[test]
    fn test_normalize_zksolc_version_with_none() {
        let normalized_version = normalize_zksolc_version(None);
        assert_eq!(normalized_version, None);
    }

    #[test]
    fn test_normalize_solc_version_with_zkvm_prefix() {
        let normalized_version =
            normalize_solc_version("zkVM-0.8.16-1.0.1".to_string(), &SolcVersionsFetcher::new());
        assert_eq!(normalized_version, "v0.8.16-1.0.1".to_string());
    }

    #[test]
    fn test_normalize_solc_version_with_zkvm_prefix_and_old_zk_version() {
        let normalized_version =
            normalize_solc_version("zkVM-0.8.16-1.0.0".to_string(), &SolcVersionsFetcher::new());
        assert_eq!(normalized_version, "v0.8.16-1.0.1".to_string());
    }

    #[test]
    fn test_normalize_solc_version_without_v_prefix() {
        let normalized_version =
            normalize_solc_version("0.8.16".to_string(), &SolcVersionsFetcher::new());
        assert_eq!(normalized_version, "v0.8.16+commit.07a7930e".to_string());
    }
}
