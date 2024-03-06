#[async_trait::async_trait]
pub trait Precondition: 'static + Send + Sync {
    /// Unique name of the precondition.
    fn name(&self) -> &'static str;

    async fn check(self: Box<Self>) -> anyhow::Result<()>;
}
