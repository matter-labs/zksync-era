use clap::{Parser, ValueEnum};
use common::PromptSelect;
use strum::{EnumIter, IntoEnumIterator};

use crate::messages::MSG_RUN_COMPONENT_PROMPT;

#[derive(Debug, Clone, Parser, Default)]
pub struct ProverRunArgs {
    #[clap(long)]
    pub component: Option<ProverComponent>,
}

#[derive(Debug, Clone, ValueEnum, strum::EnumString, EnumIter, PartialEq, Eq)]
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
        let component = self.component.clone().unwrap_or_else(|| {
            PromptSelect::new(MSG_RUN_COMPONENT_PROMPT, ProverComponent::iter()).ask()
        });

        Ok(ProverRunArgs {
            component: Some(component),
        })
    }
}
