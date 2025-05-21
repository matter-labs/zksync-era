use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

/// URL with potentially sensitive authentication info (user name and password). Has a specialized
/// `Debug` implementation to hide sensitive parts and explicit API to expose the underlying URL.
#[derive(Clone, PartialEq)]
pub struct SensitiveUrl {
    inner: Url,
    sensitive_query_params: &'static [&'static str],
}

impl fmt::Debug for SensitiveUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.inner.username().is_empty()
            && self.inner.password().is_none()
            && (self.sensitive_query_params.is_empty() || self.inner.query().is_none())
        {
            fmt::Debug::fmt(&self.inner.as_str(), formatter)
        } else {
            let mut censored_url = self.inner.clone();
            if !self.inner.username().is_empty() {
                censored_url.set_username("***").ok();
            }
            if self.inner.password().is_some() {
                censored_url.set_password(Some("***")).ok();
            }
            if self.inner.query().is_some() {
                let mut query_pairs = censored_url.query_pairs_mut();
                query_pairs.clear();
                for (name, value) in self.inner.query_pairs() {
                    query_pairs.append_pair(
                        &name,
                        if self.sensitive_query_params.contains(&name.as_ref()) {
                            "***"
                        } else {
                            &value
                        },
                    );
                }
            }
            fmt::Debug::fmt(&censored_url.as_str(), formatter)
        }
    }
}

impl From<Url> for SensitiveUrl {
    fn from(url: Url) -> Self {
        Self {
            inner: url,
            sensitive_query_params: &[],
        }
    }
}

impl FromStr for SensitiveUrl {
    type Err = url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<Url>().map(Self::from)
    }
}

impl SensitiveUrl {
    /// Sets names of query params containing sensitive data.
    #[must_use]
    pub fn with_sensitive_query_params(mut self, params: &'static [&'static str]) -> Self {
        self.sensitive_query_params = params;
        self
    }

    /// Exposes the underlying URL.
    pub fn expose_url(&self) -> &Url {
        &self.inner
    }

    /// Exposes the underlying URL string.
    pub fn expose_str(&self) -> &str {
        self.inner.as_str()
    }
}

impl Serialize for SensitiveUrl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.inner.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SensitiveUrl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Url::deserialize(deserializer).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_censoring_works_as_expected() {
        let url: SensitiveUrl = "postgres://postgres:notsecurepassword@localhost/dev"
            .parse()
            .unwrap();
        let url_debug = format!("{url:?}");
        assert_eq!(
            url_debug,
            format!("{:?}", "postgres://***:***@localhost/dev")
        );

        let url_str = "postgres://localhost/dev?application_name=app&user=postgres&password=notsecurepassword\
            &options=-c+synchronous_commit%3Doff";
        let url = url_str
            .parse::<SensitiveUrl>()
            .unwrap()
            .with_sensitive_query_params(&["user", "password"]);
        let url_debug = format!("{url:?}");
        assert_eq!(
            url_debug,
            format!("{:?}", "postgres://localhost/dev?application_name=app&user=***&password=***&options=-c+synchronous_commit%3Doff")
        );
    }
}
