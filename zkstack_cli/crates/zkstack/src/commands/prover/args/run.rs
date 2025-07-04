use std::path::Path;

use anyhow::anyhow;
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator};
use zkstack_cli_common::{Prompt, PromptSelect};
use zkstack_cli_config::ChainConfig;

use crate::{
    consts::{
        CIRCUIT_PROVER_BINARY_NAME, CIRCUIT_PROVER_DOCKER_IMAGE, COMPRESSOR_BINARY_NAME,
        COMPRESSOR_DOCKER_IMAGE, PROVER_GATEWAY_BINARY_NAME, PROVER_GATEWAY_DOCKER_IMAGE,
        PROVER_JOB_MONITOR_BINARY_NAME, PROVER_JOB_MONITOR_DOCKER_IMAGE,
        WITNESS_GENERATOR_BINARY_NAME, WITNESS_GENERATOR_DOCKER_IMAGE,
    },
    messages::{
        MSG_ROUND_SELECT_PROMPT, MSG_RUN_COMPONENT_PROMPT, MSG_WITNESS_GENERATOR_ROUND_ERR,
    },
};

#[derive(Debug, Clone, Parser, Default)]
pub struct ProverRunArgs {
    #[clap(long)]
    pub component: Option<ProverComponent>,
    #[clap(flatten)]
    pub witness_generator_args: WitnessGeneratorArgs,
    #[clap(flatten)]
    pub circuit_prover_args: CircuitProverArgs,
    #[clap(flatten)]
    pub compressor_args: CompressorArgs,
    #[clap(long)]
    pub docker: Option<bool>,
    #[clap(long)]
    pub tag: Option<String>,
}

#[derive(
    Debug, Clone, ValueEnum, strum::EnumString, EnumIter, PartialEq, Eq, Copy, strum::Display,
)]
pub enum ProverComponent {
    #[strum(to_string = "Gateway")]
    Gateway,
    #[strum(to_string = "Witness generator")]
    WitnessGenerator,
    #[strum(to_string = "CircuitProver")]
    CircuitProver,
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
            Self::CircuitProver => CIRCUIT_PROVER_DOCKER_IMAGE,
            Self::Compressor => COMPRESSOR_DOCKER_IMAGE,
            Self::ProverJobMonitor => PROVER_JOB_MONITOR_DOCKER_IMAGE,
        }
    }

    pub fn binary_name(&self) -> &'static str {
        match self {
            Self::Gateway => PROVER_GATEWAY_BINARY_NAME,
            Self::WitnessGenerator => WITNESS_GENERATOR_BINARY_NAME,
            Self::CircuitProver => CIRCUIT_PROVER_BINARY_NAME,
            Self::Compressor => COMPRESSOR_BINARY_NAME,
            Self::ProverJobMonitor => PROVER_JOB_MONITOR_BINARY_NAME,
        }
    }

    pub fn get_application_args(&self, in_docker: bool) -> anyhow::Result<Vec<String>> {
        let mut application_args = vec![];

        if (self == &Self::Compressor || self == &Self::CircuitProver) && in_docker {
            application_args.push("--gpus=all".to_string());
        }

        Ok(application_args)
    }

    pub fn get_additional_args(
        &self,
        in_docker: bool,
        args: ProverRunArgs,
        chain: &ChainConfig,
        path_to_ecosystem: &Path,
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

            let general_config = path_to_ecosystem
                .join(general_config)
                .into_os_string()
                .into_string()
                .unwrap();

            let secrets_config = path_to_ecosystem
                .join(secrets_config)
                .into_os_string()
                .into_string()
                .unwrap();

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
            Self::CircuitProver => {
                if args.circuit_prover_args.max_allocation.is_some() {
                    additional_args.push(format!(
                        "--max-allocation={}",
                        args.circuit_prover_args.max_allocation.unwrap()
                    ));
                };
                if args.circuit_prover_args.threads.is_some() {
                    additional_args.push(format!(
                        "--threads={}",
                        args.circuit_prover_args.threads.unwrap()
                    ));
                };
                if args.circuit_prover_args.light_wvg_count.is_some() {
                    additional_args.push(format!(
                        "--light-wvg-count={}",
                        args.circuit_prover_args.light_wvg_count.unwrap()
                    ));
                };
                if args.circuit_prover_args.heavy_wvg_count.is_some() {
                    additional_args.push(format!(
                        "--heavy-wvg-count={}",
                        args.circuit_prover_args.heavy_wvg_count.unwrap()
                    ));
                };
            }
            Self::Compressor => {
                if args.compressor_args.mode == CompressorMode::Fflonk {
                    additional_args.push("--fflonk=true".to_string());
                }
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
pub struct CircuitProverArgs {
    #[clap(short = 'l', long)]
    pub light_wvg_count: Option<usize>,
    #[clap(short = 'h', long)]
    pub heavy_wvg_count: Option<usize>,
    #[clap(short = 't', long)]
    pub threads: Option<usize>,
    #[clap(short = 'm', long)]
    pub max_allocation: Option<usize>,
}

impl CircuitProverArgs {
    pub fn fill_values_with_prompt(
        self,
        component: ProverComponent,
    ) -> anyhow::Result<CircuitProverArgs> {
        if component != ProverComponent::CircuitProver {
            return Ok(Self::default());
        }

        let threads = if self.light_wvg_count.is_none() && self.heavy_wvg_count.is_none() {
            Some(self.threads.unwrap_or_else(|| {
                Prompt::new("Number of CPU threads to run WVGs in parallel (to specify light & heavy WVGs put 0)")
                    .default("0")
                    .ask()
            }))
        } else {
            None
        };

        let light_wvg_count = if threads.is_none() || threads == Some(0) {
            Some(self.light_wvg_count.unwrap_or_else(|| {
                Prompt::new("Number of light WVG jobs to run in parallel")
                    .default("8")
                    .ask()
            }))
        } else {
            None
        };

        let heavy_wvg_count = if threads.is_none() || threads == Some(0) {
            Some(self.heavy_wvg_count.unwrap_or_else(|| {
                Prompt::new("Number of heavy WVG jobs to run in parallel")
                    .default("2")
                    .ask()
            }))
        } else {
            None
        };

        Ok(CircuitProverArgs {
            light_wvg_count,
            heavy_wvg_count,
            threads,
            max_allocation: self.max_allocation,
        })
    }
}

#[derive(Debug, Clone, Parser, Default)]
pub struct CompressorArgs {
    #[clap(long, default_value = "plonk")]
    pub mode: CompressorMode,
}

#[derive(
    Default,
    Debug,
    Clone,
    ValueEnum,
    EnumIter,
    strum::Display,
    PartialEq,
    Eq,
    Deserialize,
    Serialize,
)]
pub enum CompressorMode {
    Fflonk,
    #[default]
    Plonk,
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

        let circuit_prover_args = self
            .circuit_prover_args
            .fill_values_with_prompt(component)?;

        let docker = self.docker.unwrap_or_else(|| {
            Prompt::new("Do you want to run Docker image for the component?")
                .default("false")
                .ask()
        });

        let tag = self.tag.unwrap_or("latest".to_string());

        Ok(ProverRunArgs {
            component: Some(component),
            witness_generator_args,
            circuit_prover_args,
            compressor_args: self.compressor_args,
            docker: Some(docker),
            tag: Some(tag),
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
