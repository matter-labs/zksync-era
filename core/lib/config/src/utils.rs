//! Configuration-related utils.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer};
use url::Url;

#[derive(Clone, PartialEq)]
pub struct SensitiveUrl(Url);

impl fmt::Debug for SensitiveUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.username().is_empty() && self.0.password().is_none() {
            fmt::Debug::fmt(&self.0.as_str(), formatter)
        } else {
            let mut censored_url = self.0.clone();
            if !self.0.username().is_empty() {
                censored_url.set_username("***").ok();
            }
            if self.0.password().is_some() {
                censored_url.set_password(Some("***")).ok();
            }
            fmt::Debug::fmt(&censored_url.as_str(), formatter)
        }
    }
}

impl FromStr for SensitiveUrl {
    type Err = url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Url>().map(Self)
    }
}

impl SensitiveUrl {
    pub fn expose_url(&self) -> &Url {
        &self.0
    }

    pub fn expose_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<'de> Deserialize<'de> for SensitiveUrl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Url::deserialize(deserializer).map(Self)
    }
}
