use anyhow::{Context, Result};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Client,
};
use semver::Version;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub commit: Commit,
    pub zipball_url: String,
    pub tarball_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Commit {
    pub sha: String,
    pub url: String,
}

/// Fetches and sorts GitHub repository tags by semantic version
pub struct GitHubTagFetcher {
    client: Client,
    auth_token: Option<String>,
}

impl GitHubTagFetcher {
    pub fn new(auth_token: Option<String>) -> Result<Self> {
        let client = Client::new();
        Ok(Self { client, auth_token })
    }

    pub async fn get_newest_core_tags(&self, limit: Option<usize>) -> Result<Vec<Tag>> {
        let mut all_tags = Vec::new();
        let mut page = 1;

        // Set up headers
        let mut headers = HeaderMap::new();
        headers.insert(
            "Accept",
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert("User-Agent", HeaderValue::from_static("zkstack"));
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );

        if let Some(token) = &self.auth_token {
            headers.insert(
                "Authorization",
                HeaderValue::from_str(&format!("Bearer {}", token))
                    .context("Invalid authorization header")?,
            );
        }

        // Fetch all pages
        loop {
            let url = format!(
                "https://api.github.com/repos/matter-labs/zksync-era/tags?page={}&per_page=100",
                page
            );

            let response = self
                .client
                .get(&url)
                .headers(headers.clone())
                .send()
                .await?;

            if !response.status().is_success() {
                return Err(anyhow::anyhow!(
                    "GitHub API request failed with status: {}",
                    response.status()
                ));
            }

            let page_tags: Vec<Tag> = response.json().await?;
            if page_tags.is_empty() {
                break;
            }

            all_tags.extend(page_tags);
            page += 1;
        }

        // filter tag names containing "core"
        all_tags.retain(|t| t.name.contains("core"));

        // Sort tags by semantic version
        all_tags.sort_by(|a, b| {
            let version_a = clean_version(&a.name);
            let version_b = clean_version(&b.name);
            version_b.cmp(&version_a) // Reverse order (newest first)
        });

        // Apply limit if specified
        Ok(if let Some(limit) = limit {
            all_tags.into_iter().take(limit).collect()
        } else {
            all_tags
        })
    }
}

/// Cleans and parses version strings into semver::Version
fn clean_version(tag_name: &str) -> Version {
    // Remove "core-v" prefix and parse the version string
    let cleaned = tag_name.trim_start_matches("core-v");
    Version::parse(cleaned).unwrap_or_else(|_| Version::new(0, 0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_sorted_tags() -> anyhow::Result<()> {
        // Replace with your GitHub token if needed
        let auth_token = std::env::var("GITHUB_TOKEN").ok();

        let fetcher = GitHubTagFetcher::new(auth_token)?;

        // Example usage
        let tags = fetcher.get_newest_core_tags(Some(5)).await?;

        println!("\nLatest Repository Tags (sorted by version):");
        for tag in tags {
            println!("Tag: {}", tag.name);
            println!("Commit SHA: {}", tag.commit.sha);
            println!("ZIP URL: {}\n", tag.zipball_url);
        }

        Ok(())
    }
}
