use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProvingNetwork {
    Lagrange,
    Fermah,
    None,
}

impl From<ProvingNetwork> for u8 {
    fn from(network: ProvingNetwork) -> Self {
        match network {
            ProvingNetwork::Lagrange => 0,
            ProvingNetwork::Fermah => 1,
            ProvingNetwork::None => 2,
        }
    }
}

impl FromStr for ProvingNetwork {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "lagrange" => ProvingNetwork::Lagrange,
            "fermah" => ProvingNetwork::Fermah,
            "none" => ProvingNetwork::None,
        })
    }
}

impl From<ProvingNetwork> for &str {
    fn from(network: ProvingNetwork) -> Self {
        match network {
            ProvingNetwork::Lagrange => "lagrange",
            ProvingNetwork::Fermah => "fermah",
            ProvingNetwork::None => "none",
        }
    }
}
