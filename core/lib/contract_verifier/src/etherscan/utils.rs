use super::solc_versions_fetcher::SolcVersionsFetcher;

/// Normalizes the solc version to the format expected by Etherscan.
pub(super) fn normalize_solc_version(
    version: String,
    solc_versions_fetcher: &SolcVersionsFetcher,
) -> String {
    if version.contains("zkVM-") {
        return version.replace("zkVM-", "v");
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
    fn test_normalize_solc_version_with_zkvm_prefix() {
        let normalized_version =
            normalize_solc_version("zkVM-0.8.16-1.0.1".to_string(), &SolcVersionsFetcher::new());
        assert_eq!(normalized_version, "v0.8.16-1.0.1".to_string());
    }

    #[test]
    fn test_normalize_solc_version_without_v_prefix() {
        let normalized_version =
            normalize_solc_version("0.8.16".to_string(), &SolcVersionsFetcher::new());
        assert_eq!(normalized_version, "v0.8.16+commit.07a7930e".to_string());
    }
}
