use common::{github::GitHubTagFetcher, PromptSelect};

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

    Ok(PromptSelect::new("Select image", tags).ask())
}
