use std::fmt::Display;

use cliclack::Confirm;

pub struct PromptConfirm {
    inner: Confirm,
}

impl PromptConfirm {
    pub fn new(question: impl Display) -> Self {
        Self {
            inner: Confirm::new(question),
        }
    }

    pub fn default(self, default: bool) -> Self {
        Self {
            inner: self.inner.initial_value(default),
        }
    }

    pub fn ask(mut self) -> bool {
        self.inner.interact().unwrap()
    }
}
