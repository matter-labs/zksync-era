use anyhow::anyhow;
use clap::{Parser, ValueEnum};
use common::{Prompt, PromptSelect};
use config::ChainConfig;
use strum::{EnumIter, IntoEnumIterator};

use crate::{
    consts::{
        COMPRESSOR_BINARY_NAME, COMPRESSOR_DOCKER_IMAGE, PROVER_BINARY_NAME, PROVER_DOCKER_IMAGE,
        PROVER_GATEWAY_BINARY_NAME, PROVER_GATEWAY_DOCKER_IMAGE, PROVER_JOB_MONITOR_BINARY_NAME,
        PROVER_JOB_MONITOR_DOCKER_IMAGE, WITNESS_GENERATOR_BINARY_NAME,
        WITNESS_GENERATOR_DOCKER_IMAGE, WITNESS_VECTOR_GENERATOR_BINARY_NAME,
        WITNESS_VECTOR_GENERATOR_DOCKER_IMAGE,
    },
    messages::{
        MSG_ROUND_SELECT_PROMPT, MSG_RUN_COMPONENT_PROMPT, MSG_THREADS_PROMPT,
        MSG_WITNESS_GENERATOR_ROUND_ERR,
    },
};

#[derive(Debug, Clone, Parser, Default)]
pub struct ProverRunArgs {
    #[clap(long)]
    pub component: Option<ProverComponent>,
    #[clap(flatten)]
    pub witness_generator_args: WitnessGeneratorArgs,
    #[clap(flatten)]
    pub witness_vector_generator_args: WitnessVectorGeneratorArgs,
    #[clap(flatten)]
    pub fri_prover_args: FriProverRunArgs,
    #[clap(long)]
    pub docker: Option<bool>,
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
    #[strum(to_string = "ProverJobMonitor")]
    ProverJobMonitor,
}

impl ProverComponent {
    pub fn image_name(&self) -> &'static str {
        match self {
            Self::Gateway => PROVER_GATEWAY_DOCKER_IMAGE,
            Self::WitnessGenerator => WITNESS_GENERATOR_DOCKER_IMAGE,
            Self::WitnessVectorGenerator => WITNESS_VECTOR_GENERATOR_DOCKER_IMAGE,
            Self::Prover => PROVER_DOCKER_IMAGE,
            Self::Compressor => COMPRESSOR_DOCKER_IMAGE,
            Self::ProverJobMonitor => PROVER_JOB_MONITOR_DOCKER_IMAGE,
        }
    }

    pub fn binary_name(&self) -> &'static str {
        match self {
            Self::Gateway => PROVER_GATEWAY_BINARY_NAME,
            Self::WitnessGenerator => WITNESS_GENERATOR_BINARY_NAME,
            Self::WitnessVectorGenerator => WITNESS_VECTOR_GENERATOR_BINARY_NAME,
            Self::Prover => PROVER_BINARY_NAME,
            Self::Compressor => COMPRESSOR_BINARY_NAME,
            Self::ProverJobMonitor => PROVER_JOB_MONITOR_BINARY_NAME,
        }
    }

    pub fn get_application_args(&self, in_docker: bool) -> anyhow::Result<Vec<String>> {
        let mut application_args = vec![];

        if self == &Self::Prover || self == &Self::Compressor {
            if in_docker {
                application_args.push("--gpus=all".to_string());
            } else {
                application_args.push("--features=gpu".to_string());
            }
        }

        Ok(application_args)
    }

    pub fn get_additional_args(
        &self,
        in_docker: bool,
        args: ProverRunArgs,
        chain: &ChainConfig,
    ) -> anyhow::Result<Vec<String>> {
        let mut additional_args = vec![];
        if in_docker {
            additional_args.push("--config-path=/configs/general.yaml".to_string());
            additional_args.push("--secrets-path=/configs/secrets.yaml".to_string());
        } else {
            let general_config = chain
                .path_to_general_config()
                .into_os_string()
                .into_string()
                .map_err(|_| anyhow!("Failed to convert path to string"))?;
            let secrets_config = chain
                .path_to_secrets_config()
                .into_os_string()
                .into_string()
                .map_err(|_| anyhow!("Failed to convert path to string"))?;

            additional_args.push(format!("--config-path={}", general_config));
            additional_args.push(format!("--secrets-path={}", secrets_config));
        }

        match self {
            Self::WitnessGenerator => {
                additional_args.push(
                    match args
                        .witness_generator_args
                        .round
                        .expect(MSG_WITNESS_GENERATOR_ROUND_ERR)
                    {
                        WitnessGeneratorRound::AllRounds => "--all_rounds",
                        WitnessGeneratorRound::BasicCircuits => "--round=basic_circuits",
                        WitnessGeneratorRound::LeafAggregation => "--round=leaf_aggregation",
                        WitnessGeneratorRound::NodeAggregation => "--round=node_aggregation",
                        WitnessGeneratorRound::RecursionTip => "--round=recursion_tip",
                        WitnessGeneratorRound::Scheduler => "--round=scheduler",
                    }
                    .to_string(),
                );
            }
            Self::WitnessVectorGenerator => {
                additional_args.push(format!(
                    "--threads={}",
                    args.witness_vector_generator_args.threads.unwrap_or(1)
                ));
            }
            Self::Prover => {
                if args.fri_prover_args.max_allocation.is_some() {
                    additional_args.push(format!(
                        "--max-allocation={}",
                        args.fri_prover_args.max_allocation.unwrap()
                    ));
                };
            }
            _ => {}
        };

        Ok(additional_args)
    }
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

#[derive(Debug, Clone, Parser, Default)]
pub struct FriProverRunArgs {
    /// Memory allocation limit in bytes (for prover component)
    #[clap(long)]
    pub max_allocation: Option<usize>,
}

impl ProverRunArgs {
    pub fn fill_values_with_prompt(self) -> anyhow::Result<ProverRunArgs> {
        let component = self.component.unwrap_or_else(|| {
            PromptSelect::new(MSG_RUN_COMPONENT_PROMPT, ProverComponent::iter()).ask()
        });

        let witness_generator_args = self
            .witness_generator_args
            .fill_values_with_prompt(component)?;

        let witness_vector_generator_args = self
            .witness_vector_generator_args
            .fill_values_with_prompt(component)?;

        let docker = self.docker.unwrap_or_else(|| {
            Prompt::new("Do you want to run Docker image for the component?")
                .default("false")
                .ask()
        });

        Ok(ProverRunArgs {
            component: Some(component),
            witness_generator_args,
            witness_vector_generator_args,
            fri_prover_args: self.fri_prover_args,
            docker: Some(docker),
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
