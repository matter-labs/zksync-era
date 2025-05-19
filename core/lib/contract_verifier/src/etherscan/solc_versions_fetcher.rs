use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use reqwest::Client;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Build {
    version: String,
    long_version: String,
    prerelease: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Response {
    builds: Vec<Build>,
}

const SOLC_BUILDS_URL: &str =
    "https://raw.githubusercontent.com/ethereum/solc-bin/refs/heads/gh-pages/bin/list.json";

// This is a hardcoded list of solc versions that are supported by Etherscan
const SOLC_VERSIONS: &[(&str, &str)] = &[
    ("0.4.11", "0.4.11+commit.68ef5810"),
    ("0.4.12", "0.4.12+commit.194ff033"),
    ("0.4.13", "0.4.13+commit.0fb4cb1a"),
    ("0.4.14", "0.4.14+commit.c2215d46"),
    ("0.4.15", "0.4.15+commit.8b45bddb"),
    ("0.4.16", "0.4.16+commit.d7661dd9"),
    ("0.4.17", "0.4.17+commit.bdeb9e52"),
    ("0.4.18", "0.4.18+commit.9cf6e910"),
    ("0.4.19", "0.4.19+commit.c4cbbb05"),
    ("0.4.20", "0.4.20+commit.3155dd80"),
    ("0.4.21", "0.4.21+commit.dfe3193c"),
    ("0.4.22", "0.4.22+commit.4cb486ee"),
    ("0.4.23", "0.4.23+commit.124ca40d"),
    ("0.4.24", "0.4.24+commit.e67f0147"),
    ("0.4.25", "0.4.25+commit.59dbf8f1"),
    ("0.4.26", "0.4.26+commit.4563c3fc"),
    ("0.5.0", "0.5.0+commit.1d4f565a"),
    ("0.5.1", "0.5.1+commit.c8a2cb62"),
    ("0.5.2", "0.5.2+commit.1df8f40c"),
    ("0.5.3", "0.5.3+commit.10d17f24"),
    ("0.5.4", "0.5.4+commit.9549d8ff"),
    ("0.5.5", "0.5.5+commit.47a71e8f"),
    ("0.5.6", "0.5.6+commit.b259423e"),
    ("0.5.7", "0.5.7+commit.6da8b019"),
    ("0.5.8", "0.5.8+commit.23d335f2"),
    ("0.5.9", "0.5.9+commit.c68bc34e"),
    ("0.5.10", "0.5.10+commit.5a6ea5b1"),
    ("0.5.11", "0.5.11+commit.22be8592"),
    ("0.5.12", "0.5.12+commit.7709ece9"),
    ("0.5.13", "0.5.13+commit.5b0b510c"),
    ("0.5.14", "0.5.14+commit.01f1aaa4"),
    ("0.5.15", "0.5.15+commit.6a57276f"),
    ("0.5.16", "0.5.16+commit.9c3226ce"),
    ("0.5.17", "0.5.17+commit.d19bba13"),
    ("0.6.0", "0.6.0+commit.26b70077"),
    ("0.6.1", "0.6.1+commit.e6f7d5a4"),
    ("0.6.2", "0.6.2+commit.bacdbe57"),
    ("0.6.3", "0.6.3+commit.8dda9521"),
    ("0.6.4", "0.6.4+commit.1dca32f3"),
    ("0.6.5", "0.6.5+commit.f956cc89"),
    ("0.6.6", "0.6.6+commit.6c089d02"),
    ("0.6.7", "0.6.7+commit.b8d736ae"),
    ("0.6.8", "0.6.8+commit.0bbfe453"),
    ("0.6.9", "0.6.9+commit.3e3065ac"),
    ("0.6.10", "0.6.10+commit.00c0fcaf"),
    ("0.6.11", "0.6.11+commit.5ef660b1"),
    ("0.6.12", "0.6.12+commit.27d51765"),
    ("0.7.0", "0.7.0+commit.9e61f92b"),
    ("0.7.1", "0.7.1+commit.f4a555be"),
    ("0.7.2", "0.7.2+commit.51b20bc0"),
    ("0.7.3", "0.7.3+commit.9bfce1f6"),
    ("0.7.4", "0.7.4+commit.3f05b770"),
    ("0.7.5", "0.7.5+commit.eb77ed08"),
    ("0.7.6", "0.7.6+commit.7338295f"),
    ("0.8.0", "0.8.0+commit.c7dfd78e"),
    ("0.8.1", "0.8.1+commit.df193b15"),
    ("0.8.2", "0.8.2+commit.661d1103"),
    ("0.8.3", "0.8.3+commit.8d00100c"),
    ("0.8.4", "0.8.4+commit.c7e474f2"),
    ("0.8.5", "0.8.5+commit.a4f2e591"),
    ("0.8.6", "0.8.6+commit.11564f7e"),
    ("0.8.7", "0.8.7+commit.e28d00a7"),
    ("0.8.8", "0.8.8+commit.dddeac2f"),
    ("0.8.9", "0.8.9+commit.e5eed63a"),
    ("0.8.10", "0.8.10+commit.fc410830"),
    ("0.8.11", "0.8.11+commit.d7f03943"),
    ("0.8.12", "0.8.12+commit.f00d7308"),
    ("0.8.13", "0.8.13+commit.abaa5c0e"),
    ("0.8.14", "0.8.14+commit.80d49f37"),
    ("0.8.15", "0.8.15+commit.e14f2714"),
    ("0.8.16", "0.8.16+commit.07a7930e"),
    ("0.8.17", "0.8.17+commit.8df45f5f"),
    ("0.8.18", "0.8.18+commit.87f61d96"),
    ("0.8.19", "0.8.19+commit.7dd6d404"),
    ("0.8.20", "0.8.20+commit.a1b79de6"),
    ("0.8.21", "0.8.21+commit.d9974bed"),
    ("0.8.22", "0.8.22+commit.4fc1097e"),
    ("0.8.23", "0.8.23+commit.f704f362"),
    ("0.8.24", "0.8.24+commit.e11b9ed9"),
    ("0.8.25", "0.8.25+commit.b61c2a91"),
    ("0.8.26", "0.8.26+commit.8a97fa7a"),
    ("0.8.27", "0.8.27+commit.40a35a09"),
    ("0.8.28", "0.8.28+commit.7893614a"),
    ("0.8.29", "0.8.29+commit.ab55807c"),
];

/// Fetches solc long versions from the official solc-bin repository. Long build versions are required for verifying
/// contracts on Etherscan.
#[derive(Debug, Clone)]
pub(super) struct SolcVersionsFetcher {
    http_client: Client,
    solc_long_versions: HashMap<String, String>,
    last_update: Option<Instant>,
}

impl Default for SolcVersionsFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl SolcVersionsFetcher {
    /// Creates a new instance of SolcVersionsFetcher. Populates the default list of solc versions.
    pub fn new() -> Self {
        let client = Client::builder()
            // Timeout for establishing connection
            .connect_timeout(Self::connect_timeout())
            // Total timeout for the request
            .timeout(Self::request_timeout())
            .build()
            .expect("Failed to create a reqwest client for SolcVersionsFetcher");

        let solc_long_versions: HashMap<String, String> = SOLC_VERSIONS
            .iter()
            .map(|(s, l)| (s.to_string(), l.to_string()))
            .collect();

        Self {
            http_client: client,
            solc_long_versions,
            last_update: None,
        }
    }

    fn connect_timeout() -> Duration {
        Duration::from_secs(10)
    }

    fn request_timeout() -> Duration {
        Duration::from_secs(20)
    }

    fn update_interval() -> Duration {
        Duration::from_secs(60 * 30) // 30 minutes
    }

    /// Updates solc long versions from the official solc-bin repository. Internally cached so the actual update will
    /// happen only once per update interval.
    pub async fn update_versions(&mut self) -> anyhow::Result<()> {
        // Skip update if the update interval hasn't passed yet
        if let Some(time) = self.last_update {
            if time.elapsed() < Self::update_interval() {
                return Ok(());
            }
        }

        let response = self.http_client.get(SOLC_BUILDS_URL).send().await?;
        let response: Response = response.json().await?;
        for build in response.builds {
            // Skip prerelease versions
            if build.prerelease.is_none() {
                self.solc_long_versions
                    .insert(build.version, build.long_version);
            }
        }
        self.last_update = Some(Instant::now());
        tracing::info!("Successfully updated solc long versions");
        Ok(())
    }

    /// Returns the long solc version. E.g. 0.8.28 -> 0.8.28+commit.7893614a
    pub fn get_solc_long_version(&self, solc_version: &str) -> Option<String> {
        self.solc_long_versions
            .get(solc_version)
            .map(|s| s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solc_builds_fetcher_has_predefined_versions() {
        let fetcher = SolcVersionsFetcher::new();
        assert_eq!(fetcher.solc_long_versions.len(), 84);
        assert_eq!(
            fetcher.get_solc_long_version("0.4.11"),
            Some("0.4.11+commit.68ef5810".to_string())
        );
        assert_eq!(
            fetcher.get_solc_long_version("0.8.29"),
            Some("0.8.29+commit.ab55807c".to_string())
        );
    }
}
