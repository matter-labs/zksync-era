use async_trait::async_trait;

/// Convenience trait to tie together all task wrappers.
#[async_trait]
pub trait Task {
    async fn run(mut self) -> anyhow::Result<()>;
}
