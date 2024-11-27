use common::{github::GitHubTagFetcher, PromptSelect};

use crate::messages::MSG_SERVER_SELECT_DOCKER_IMAGE_TAG;

pub async fn select_tag() -> anyhow::Result<String> {
    let fetcher = GitHubTagFetcher::new(None)?;
    let gh_tags = fetcher.get_newest_core_tags(Some(5)).await?;

    let tags: Vec<String> = std::iter::once("latest".to_string())
        .chain(
            gh_tags
                .iter()
                .map(|r| r.name.trim_start_matches("core-").to_string()),
        )
        .collect();

    Ok(PromptSelect::new(MSG_SERVER_SELECT_DOCKER_IMAGE_TAG, tags).ask())
}
