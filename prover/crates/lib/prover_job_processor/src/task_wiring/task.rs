use async_trait::async_trait;

#[async_trait]
pub trait Task {
    async fn run(mut self) -> anyhow::Result<()>;
}
