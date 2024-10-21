use cliclack::Select;

pub struct PromptSelect<T> {
    inner: Select<T>,
}

impl<T> PromptSelect<T>
where
    T: Clone + Eq,
{
    pub fn new<I>(question: &str, items: I) -> Self
    where
        T: ToString + Clone,
        I: IntoIterator<Item = T>,
    {
        let items = items
            .into_iter()
            .map(|item| {
                let label = item.to_string();
                let hint = "";
                (item, label, hint)
            })
            .collect::<Vec<_>>();
        Self {
            inner: Select::new(question).items(&items),
        }
    }

    pub fn ask(mut self) -> T {
        self.inner.interact().unwrap()
    }
}
