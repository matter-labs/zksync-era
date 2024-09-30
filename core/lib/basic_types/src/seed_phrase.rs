use std::str::FromStr;

use secrecy::{ExposeSecret, Secret};

#[derive(Debug, Clone)]
pub struct SeedPhrase(pub Secret<String>);

impl PartialEq for SeedPhrase {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret().eq(other.0.expose_secret())
    }
}

impl FromStr for SeedPhrase {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(SeedPhrase(s.parse()?))
    }
}
