use std::str::FromStr;

use secrecy::{ExposeSecret, Secret};

#[derive(Debug, Clone)]
pub struct APIKey(pub Secret<String>);

impl PartialEq for APIKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret().eq(other.0.expose_secret())
    }
}

impl FromStr for APIKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(APIKey(s.parse()?))
    }
}
