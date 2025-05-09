use secrecy::{ExposeSecret, SecretString};

#[derive(Debug, Clone)]
pub struct SeedPhrase(pub SecretString);

impl From<SecretString> for SeedPhrase {
    fn from(s: SecretString) -> Self {
        Self(s)
    }
}

impl ExposeSecret<str> for SeedPhrase {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

#[derive(Debug, Clone)]
pub struct PrivateKey(pub SecretString);

impl From<SecretString> for PrivateKey {
    fn from(s: SecretString) -> Self {
        Self(s)
    }
}

impl ExposeSecret<str> for PrivateKey {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

#[derive(Debug, Clone)]
pub struct APIKey(pub SecretString);

impl From<SecretString> for APIKey {
    fn from(s: SecretString) -> Self {
        Self(s)
    }
}

impl ExposeSecret<str> for APIKey {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}
