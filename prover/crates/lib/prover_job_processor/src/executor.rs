/// Executor trait
pub trait Executor: Send + Sync + 'static {
    type Input: Send;
    type Output: Send;
    type Metadata: Send;

    fn execute(&self, input: Self::Input) -> anyhow::Result<Self::Output>;
}
