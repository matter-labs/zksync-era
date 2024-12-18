use secrecy::SecretString;

#[derive(Debug, Clone)]
pub struct SeedPhrase(pub SecretString);

impl From<SecretString> for SeedPhrase {
    fn from(s: SecretString) -> Self {
        Self(s)
    }
}

#[derive(Debug, Clone)]
pub struct PrivateKey(pub SecretString);

impl From<SecretString> for PrivateKey {
    fn from(s: SecretString) -> Self {
        Self(s)
    }
}

#[derive(Debug, Clone)]
pub struct APIKey(pub SecretString);

impl From<SecretString> for APIKey {
    fn from(s: SecretString) -> Self {
        Self(s)
    }
}
