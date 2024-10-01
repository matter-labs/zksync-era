pub trait Executor {
    type Input;
    type Output;

    fn execute(input: Self::Input) -> anyhow::Result<Self::Output>;
}
