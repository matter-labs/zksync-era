use clap::{Parser, ValueEnum};
use common::{Prompt, PromptSelect};
use strum::{EnumIter, IntoEnumIterator};

use crate::messages::{MSG_ROUND_SELECT_PROMPT, MSG_RUN_COMPONENT_PROMPT, MSG_THREADS_PROMPT};

#[derive(Debug, Clone, Parser, Default)]
pub struct ProverRunArgs {
    #[clap(long)]
    pub component: Option<ProverComponent>,
    #[clap(flatten)]
    pub witness_generator_args: WitnessGeneratorArgs,
    #[clap(flatten)]
    pub witness_vector_generator_args: WitnessVectorGeneratorArgs,
}

#[derive(
    Debug, Clone, ValueEnum, strum::EnumString, EnumIter, PartialEq, Eq, Copy, strum::Display,
)]
pub enum ProverComponent {
    #[strum(to_string = "Gateway")]
    Gateway,
    #[strum(to_string = "Witness generator")]
    WitnessGenerator,
    #[strum(to_string = "Witness vector generator")]
    WitnessVectorGenerator,
    #[strum(to_string = "Prover")]
    Prover,
    #[strum(to_string = "Compressor")]
    Compressor,
}

#[derive(Debug, Clone, Parser, Default)]
pub struct WitnessGeneratorArgs {
    #[clap(long)]
    pub round: Option<WitnessGeneratorRound>,
}

#[derive(Debug, Clone, ValueEnum, strum::EnumString, EnumIter, PartialEq, Eq, strum::Display)]
pub enum WitnessGeneratorRound {
    #[strum(to_string = "All rounds")]
    AllRounds,
    #[strum(to_string = "Basic circuits")]
    BasicCircuits,
    #[strum(to_string = "Leaf aggregation")]
    LeafAggregation,
    #[strum(to_string = "Node aggregation")]
    NodeAggregation,
    #[strum(to_string = "Recursion tip")]
    RecursionTip,
    #[strum(to_string = "Scheduler")]
    Scheduler,
}

#[derive(Debug, Clone, Parser, Default)]
pub struct WitnessVectorGeneratorArgs {
    #[clap(long)]
    pub threads: Option<usize>,
}

impl WitnessVectorGeneratorArgs {
    fn fill_values_with_prompt(&self, component: ProverComponent) -> anyhow::Result<Self> {
        if component != ProverComponent::WitnessVectorGenerator {
            return Ok(Self::default());
        }

        let threads = self
            .threads
            .unwrap_or_else(|| Prompt::new(MSG_THREADS_PROMPT).default("1").ask());

        Ok(Self {
            threads: Some(threads),
        })
    }
}

impl ProverRunArgs {
    pub fn fill_values_with_prompt(&self) -> anyhow::Result<ProverRunArgs> {
        let component = self.component.unwrap_or_else(|| {
            PromptSelect::new(MSG_RUN_COMPONENT_PROMPT, ProverComponent::iter()).ask()
        });

        let witness_generator_args = self
            .witness_generator_args
            .fill_values_with_prompt(component)?;

        let witness_vector_generator_args = self
            .witness_vector_generator_args
            .fill_values_with_prompt(component)?;

        Ok(ProverRunArgs {
            component: Some(component),
            witness_generator_args,
            witness_vector_generator_args,
        })
    }
}

impl WitnessGeneratorArgs {
    pub fn fill_values_with_prompt(
        &self,
        component: ProverComponent,
    ) -> anyhow::Result<WitnessGeneratorArgs> {
        if component != ProverComponent::WitnessGenerator {
            return Ok(Self::default());
        }

        let round = self.round.clone().unwrap_or_else(|| {
            PromptSelect::new(MSG_ROUND_SELECT_PROMPT, WitnessGeneratorRound::iter()).ask()
        });

        Ok(WitnessGeneratorArgs { round: Some(round) })
    }
}
