use std::str::FromStr;

use cliclack::{Input, Validate};

pub struct Prompt {
    inner: Input,
}

impl Prompt {
    pub fn new(question: &str) -> Self {
        Self {
            inner: Input::new(question),
        }
    }

    pub fn allow_empty(mut self) -> Self {
        self.inner = self.inner.required(false);
        self
    }

    pub fn default(mut self, default: &str) -> Self {
        self.inner = self.inner.default_input(default);
        self
    }

    pub fn validate_with<F>(mut self, f: F) -> Self
    where
        F: Validate<String> + 'static,
        F::Err: ToString,
    {
        self.inner = self.inner.validate(f);
        self
    }

    pub fn ask<T>(mut self) -> T
    where
        T: FromStr,
    {
        self.inner.interact().unwrap()
    }
}
