pub trait Executor {
    type Input;
    type Output;

    fn execute(&self, input: Self::Input) -> anyhow::Result<Self::Output>;
}
