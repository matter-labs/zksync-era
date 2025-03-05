use secrecy::{ExposeSecret, SecretString};

#[derive(Debug, Clone)]
pub struct SeedPhrase(pub SecretString);

impl PartialEq for SeedPhrase {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret().eq(other.0.expose_secret())
    }
}

impl From<String> for SeedPhrase {
    fn from(s: String) -> Self {
        Self(SecretString::from(s))
    }
}

impl From<&str> for SeedPhrase {
    fn from(s: &str) -> Self {
        Self(SecretString::from(s))
    }
}

#[derive(Debug, Clone)]
pub struct PrivateKey(pub SecretString);

impl PartialEq for PrivateKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret().eq(other.0.expose_secret())
    }
}

impl From<String> for PrivateKey {
    fn from(s: String) -> Self {
        Self(SecretString::from(s))
    }
}

impl From<&str> for PrivateKey {
    fn from(s: &str) -> Self {
        Self(SecretString::from(s))
    }
}

#[derive(Debug, Clone)]
pub struct APIKey(pub SecretString);

impl PartialEq for APIKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret().eq(other.0.expose_secret())
    }
}

impl From<String> for APIKey {
    fn from(s: String) -> Self {
        Self(SecretString::from(s))
    }
}

impl From<&str> for APIKey {
    fn from(s: &str) -> Self {
        Self(SecretString::from(s))
    }
}
