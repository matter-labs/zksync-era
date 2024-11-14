/// Executor trait
pub trait Executor: Send + Sync + 'static {
    type Input: Send;
    type Output: Send;
    type Metadata: Send + Clone;

    fn execute(&self, input: Self::Input, metadata: Self::Metadata) -> anyhow::Result<Self::Output>;
}
