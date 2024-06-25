use clap::{Parser, ValueEnum};
use common::PromptConfirm;
use strum::{EnumIter, IntoEnumIterator};

use crate::messages::msg_run_prover_component_prompt;

#[derive(Debug, Clone, Parser, Default)]
pub struct ProverRunArgs {
    #[clap(long, value_delimiter = ',')]
    pub components: Option<Vec<ProverComponent>>,
}

#[derive(Debug, Clone, ValueEnum, strum::EnumString, EnumIter)]
pub enum ProverComponent {
    Gateway,
    WitnessGenerator,
    WitnessVectorGenerator,
    Prover,
    Compressor,
}

impl std::fmt::Display for ProverComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProverComponent::Gateway => write!(f, "gateway"),
            ProverComponent::WitnessGenerator => write!(f, "witness generator"),
            ProverComponent::WitnessVectorGenerator => write!(f, "witness vector generator"),
            ProverComponent::Prover => write!(f, "prover"),
            ProverComponent::Compressor => write!(f, "compressor"),
        }
    }
}

impl ProverRunArgs {
    pub fn fill_values_with_prompt(&self) -> anyhow::Result<ProverRunArgs> {
        let components = self.components.clone().unwrap_or_else(|| {
            let mut c = vec![];
            for component in ProverComponent::iter() {
                if PromptConfirm::new(msg_run_prover_component_prompt(&component.to_string()))
                    .default(true)
                    .ask()
                {
                    c.push(component);
                }
            }
            c
        });

        Ok(ProverRunArgs {
            components: Some(components),
        })
    }
}
