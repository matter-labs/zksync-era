use clap::{Parser, ValueEnum};
use common::{Prompt, PromptSelect};
use config::{ChainConfig, EcosystemConfig};
use strum::{EnumIter, IntoEnumIterator};
use xshell::Shell;

use crate::{
    commands::prover::utils::get_link_to_prover,
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
            Self::Gateway => "matterlabs/prover-fri-gateway:latest2.0",
            Self::WitnessGenerator => "matterlabs/witness-generator:latest2.0",
            Self::WitnessVectorGenerator => "matterlabs/witness-vector-generator:latest2.0",
            Self::Prover => "matterlabs/prover-gpu-fri:latest2.0",
            Self::Compressor => "matterlabs/proof-fri-gpu-compressor:latest2.0",
            Self::ProverJobMonitor => "matterlabs/prover-job-monitor:latest2.0",
        }
    }

    pub fn binary_name(&self) -> &'static str {
        match self {
            Self::Gateway => "zksync_prover_fri_gateway",
            Self::WitnessGenerator => "zksync_witness_generator",
            Self::WitnessVectorGenerator => "zksync_witness_vector_generator",
            Self::Prover => "zksync_prover_fri",
            Self::Compressor => "zksync_proof_fri_compressor",
            Self::ProverJobMonitor => "zksync_prover_job_monitor",
        }
    }

    pub fn get_application_args(
        &self,
        in_docker: bool,
        chain: &ChainConfig,
        shell: &Shell,
    ) -> anyhow::Result<Option<String>> {
        let path_to_configs = chain.configs.clone();
        let ecosystem_config = EcosystemConfig::from_file(shell)?;
        let path_to_prover = get_link_to_prover(&ecosystem_config);

        let mut application_args = match in_docker{
            true => format!("--net=host -v {path_to_prover:?}/data/keys:/prover/data/keys -v {path_to_prover:?}/artifacts:/artifacts -v {path_to_configs:?}:/configs"),
            false => "".to_string(),
        };

        if self == &Self::Prover || self == &Self::Compressor {
            if in_docker {
                application_args += " --gpus=all";
            } else {
                application_args += "--features gpu";
            }
        }

        if application_args.clone().is_empty() {
            Ok(None)
        } else {
            Ok(Some(application_args))
        }
    }

    pub fn get_additional_args(
        &self,
        args: ProverRunArgs,
        chain: &ChainConfig,
    ) -> anyhow::Result<Option<String>> {
        let general_config = chain.path_to_general_config();
        let secrets_config = chain.path_to_secrets_config();

        let mut additional_args =
            format!("--config-path={general_config:?} --secrets-path={secrets_config:?}");

        match self {
            Self::WitnessGenerator => {
                additional_args += match args
                    .witness_generator_args
                    .round
                    .expect(MSG_WITNESS_GENERATOR_ROUND_ERR)
                {
                    WitnessGeneratorRound::AllRounds => " --all_rounds",
                    WitnessGeneratorRound::BasicCircuits => " --round=basic_circuits",
                    WitnessGeneratorRound::LeafAggregation => " --round=leaf_aggregation",
                    WitnessGeneratorRound::NodeAggregation => " --round=node_aggregation",
                    WitnessGeneratorRound::RecursionTip => " --round=recursion_tip",
                    WitnessGeneratorRound::Scheduler => " --round=scheduler",
                };
            }
            Self::WitnessVectorGenerator => {
                additional_args += format!(
                    " --threads={}",
                    args.witness_vector_generator_args.threads.unwrap_or(1)
                )
                .as_str();
            }
            Self::Prover => {
                if args.fri_prover_args.max_allocation.is_some() {
                    additional_args += format!(
                        " --max-allocation={}",
                        args.fri_prover_args.max_allocation.unwrap()
                    )
                    .as_str();
                };
            }
            _ => {}
        };

        Ok(Some(additional_args))
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
