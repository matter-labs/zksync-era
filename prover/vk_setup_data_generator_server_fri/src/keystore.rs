use std::{fs, fs::File, io::Read};

use anyhow::Context as _;
use circuit_definitions::{
    boojum::cs::implementations::setup::FinalizationHintsForProver,
    circuit_definitions::{
        aux_layer::ZkSyncSnarkWrapperVK,
        base_layer::ZkSyncBaseLayerVerificationKey,
        recursion_layer::{ZkSyncRecursionLayerStorageType, ZkSyncRecursionLayerVerificationKey},
    },
    zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
};
use serde::{Deserialize, Serialize};
use zkevm_test_harness::data_source::{in_memory_data_source::InMemoryDataSource, SetupDataSource};
use zksync_config::configs::FriProverConfig;
use zksync_env_config::FromEnv;
use zksync_prover_fri_types::ProverServiceDataKey;
use zksync_types::basic_fri_types::AggregationRound;

#[cfg(feature = "gpu")]
use crate::GoldilocksGpuProverSetupData;
use crate::GoldilocksProverSetupData;

pub enum ProverServiceDataType {
    VerificationKey,
    SetupData,
    FinalizationHints,
    SnarkVerificationKey,
}

/// Key store manages all the prover keys.
/// There are 2 types:
/// - small verification, finalization keys (used only during verification)
/// - large setup keys, used during proving.
pub struct Keystore {
    /// Directory to store all the small keys.
    basedir: String,
    /// Directory to store large setup keys.
    setup_data_path: Option<String>,
}

fn get_base_path_from_env() -> String {
    let zksync_home = std::env::var("ZKSYNC_HOME").unwrap_or_else(|_| "/".into());
    format!(
        "{}/prover/vk_setup_data_generator_server_fri/data",
        zksync_home
    )
}

impl Default for Keystore {
    fn default() -> Self {
        Self {
            basedir: get_base_path_from_env(),
            setup_data_path: Some(
                FriProverConfig::from_env()
                    .expect("FriProverConfig::from_env()")
                    .setup_data_path,
            ),
        }
    }
}

impl Keystore {
    /// Base-dir is the location of smaller keys (like verification keys and finalization hints).
    /// Setup data path is used for the large setup keys.
    pub fn new(basedir: String, setup_data_path: String) -> Self {
        Keystore {
            basedir,
            setup_data_path: Some(setup_data_path),
        }
    }
    pub fn new_with_optional_setup_path(basedir: String, setup_data_path: Option<String>) -> Self {
        Keystore {
            basedir,
            setup_data_path,
        }
    }

    pub fn get_base_path(&self) -> &str {
        &self.basedir
    }

    fn get_file_path(
        &self,
        key: ProverServiceDataKey,
        service_data_type: ProverServiceDataType,
    ) -> String {
        let name = match key.round {
            AggregationRound::BasicCircuits => {
                format!("basic_{}", key.circuit_id)
            }
            AggregationRound::LeafAggregation => {
                format!("leaf_{}", key.circuit_id)
            }
            AggregationRound::NodeAggregation => "node".to_string(),
            AggregationRound::Scheduler => "scheduler".to_string(),
        };
        match service_data_type {
            ProverServiceDataType::VerificationKey => {
                format!("{}/verification_{}_key.json", self.basedir, name)
            }
            ProverServiceDataType::SetupData => {
                format!(
                    "{}/setup_{}_data.bin",
                    self.setup_data_path
                        .as_ref()
                        .expect("Setup data path not set"),
                    name
                )
            }
            ProverServiceDataType::FinalizationHints => {
                format!("{}/finalization_hints_{}.bin", self.basedir, name)
            }
            ProverServiceDataType::SnarkVerificationKey => {
                format!("{}/snark_verification_{}_key.json", self.basedir, name)
            }
        }
    }

    fn load_json_from_file<T: for<'a> Deserialize<'a>>(filepath: String) -> anyhow::Result<T> {
        let text = std::fs::read_to_string(&filepath)
            .with_context(|| format!("Failed reading verification key from path: {filepath}"))?;
        serde_json::from_str::<T>(&text)
            .with_context(|| format!("Failed deserializing verification key from path: {filepath}"))
    }
    fn save_json_pretty<T: Serialize>(filepath: String, data: &T) -> anyhow::Result<()> {
        std::fs::write(&filepath, serde_json::to_string_pretty(data).unwrap())
            .with_context(|| format!("writing to '{filepath}' failed"))
    }

    fn load_bincode_from_file<T: for<'a> Deserialize<'a>>(filepath: String) -> anyhow::Result<T> {
        let mut file = File::open(filepath.clone())
            .with_context(|| format!("Failed reading setup-data from path: {filepath:?}"))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).with_context(|| {
            format!("Failed reading setup-data to buffer from path: {filepath:?}")
        })?;
        bincode::deserialize::<T>(&buffer)
            .with_context(|| format!("Failed deserializing setup-data at path: {filepath:?}"))
    }

    ///
    ///   Verification keys
    ///

    pub fn load_base_layer_verification_key(
        &self,
        circuit_type: u8,
    ) -> anyhow::Result<ZkSyncBaseLayerVerificationKey> {
        Self::load_json_from_file(self.get_file_path(
            ProverServiceDataKey::new(circuit_type, AggregationRound::BasicCircuits),
            ProverServiceDataType::VerificationKey,
        ))
    }

    pub fn load_recursive_layer_verification_key(
        &self,
        circuit_type: u8,
    ) -> anyhow::Result<ZkSyncRecursionLayerVerificationKey> {
        Self::load_json_from_file(self.get_file_path(
            ProverServiceDataKey::new_recursive(circuit_type),
            ProverServiceDataType::VerificationKey,
        ))
    }

    pub fn save_base_layer_verification_key(
        &self,
        vk: ZkSyncBaseLayerVerificationKey,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new(vk.numeric_circuit_type(), AggregationRound::BasicCircuits),
            ProverServiceDataType::VerificationKey,
        );
        tracing::info!("saving basic verification key to: {}", filepath);
        Self::save_json_pretty(filepath, &vk)
    }

    pub fn save_recursive_layer_verification_key(
        &self,
        vk: ZkSyncRecursionLayerVerificationKey,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::new_recursive(vk.numeric_circuit_type()),
            ProverServiceDataType::VerificationKey,
        );
        tracing::info!("saving recursive layer verification key to: {}", filepath);
        Self::save_json_pretty(filepath, &vk)
    }

    ///
    /// Finalization hints
    ///

    pub fn save_finalization_hints(
        &self,
        key: ProverServiceDataKey,
        hint: &FinalizationHintsForProver,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(key.clone(), ProverServiceDataType::FinalizationHints);

        tracing::info!("saving finalization hints for {:?} to: {}", key, filepath);
        let serialized =
            bincode::serialize(&hint).context("Failed to serialize finalization hints")?;
        fs::write(filepath, serialized).context("Failed to write finalization hints to file")
    }

    pub fn load_finalization_hints(
        &self,
        key: ProverServiceDataKey,
    ) -> anyhow::Result<FinalizationHintsForProver> {
        let mut key = key;
        // For `NodeAggregation` round we have only 1 finalization hints for all circuit type.
        // TODO: is this needed??
        if key.round == AggregationRound::NodeAggregation {
            key.circuit_id = ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8;
        }
        Self::load_bincode_from_file(
            self.get_file_path(key, ProverServiceDataType::FinalizationHints),
        )
    }

    ///
    ///   Snark wrapper
    ///

    /// Loads snark verification key
    // For snark wrapper - we're actually returning a raw serialized string, and the parsing happens
    // on the reader's side (in proof compressor).
    // This way, we avoid including the old 1.3.3 test harness to our main library.
    pub fn load_snark_verification_key(&self) -> anyhow::Result<String> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::SnarkVerificationKey,
        );
        std::fs::read_to_string(&filepath)
            .with_context(|| format!("Failed reading Snark verification key from path: {filepath}"))
    }

    pub fn save_snark_verification_key(&self, vk: ZkSyncSnarkWrapperVK) -> anyhow::Result<()> {
        let filepath = self.get_file_path(
            ProverServiceDataKey::snark(),
            ProverServiceDataType::SnarkVerificationKey,
        );
        tracing::info!("saving snark verification key to: {}", filepath);
        Self::save_json_pretty(filepath, &vk.into_inner())
    }

    ///
    /// Setup keys
    ///

    pub fn load_cpu_setup_data_for_circuit_type(
        &self,
        key: ProverServiceDataKey,
    ) -> anyhow::Result<GoldilocksProverSetupData> {
        let filepath = self.get_file_path(key.clone(), ProverServiceDataType::SetupData);

        let mut file = File::open(filepath.clone())
            .with_context(|| format!("Failed reading setup-data from path: {filepath:?}"))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).with_context(|| {
            format!("Failed reading setup-data to buffer from path: {filepath:?}")
        })?;
        tracing::info!("loading {:?} setup data from path: {}", key, filepath);
        bincode::deserialize::<GoldilocksProverSetupData>(&buffer).with_context(|| {
            format!("Failed deserializing setup-data at path: {filepath:?} for circuit: {key:?}")
        })
    }

    #[cfg(feature = "gpu")]
    pub fn load_gpu_setup_data_for_circuit_type(
        &self,
        key: ProverServiceDataKey,
    ) -> anyhow::Result<GoldilocksGpuProverSetupData> {
        let filepath = self.get_file_path(key.clone(), ProverServiceDataType::SetupData);

        let mut file = File::open(filepath.clone())
            .with_context(|| format!("Failed reading setup-data from path: {filepath:?}"))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).with_context(|| {
            format!("Failed reading setup-data to buffer from path: {filepath:?}")
        })?;
        tracing::info!("loading {:?} setup data from path: {}", key, filepath);
        bincode::deserialize::<GoldilocksGpuProverSetupData>(&buffer).with_context(|| {
            format!("Failed deserializing setup-data at path: {filepath:?} for circuit: {key:?}")
        })
    }

    pub fn save_setup_data_for_circuit_type(
        &self,
        key: ProverServiceDataKey,
        serialized_setup_data: &Vec<u8>,
    ) -> anyhow::Result<()> {
        let filepath = self.get_file_path(key.clone(), ProverServiceDataType::SetupData);
        tracing::info!("saving {:?} setup data to: {}", key, filepath);
        std::fs::write(filepath.clone(), serialized_setup_data)
            .with_context(|| format!("Failed saving setup-data at path: {filepath:?}"))
    }

    /// Loads all the verification keys into the Data Source.
    /// Keys are loaded from the default 'base path' files.
    pub fn load_keys_to_data_source(&self) -> anyhow::Result<InMemoryDataSource> {
        let mut data_source = InMemoryDataSource::new();
        for base_circuit_type in
            (BaseLayerCircuitType::VM as u8)..=(BaseLayerCircuitType::L1MessagesHasher as u8)
        {
            data_source
                .set_base_layer_vk(self.load_base_layer_verification_key(base_circuit_type)?)
                .unwrap();
        }

        for circuit_type in ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8
            ..=ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8
        {
            data_source
                .set_recursion_layer_vk(self.load_recursive_layer_verification_key(circuit_type)?)
                .unwrap();
        }
        data_source
            .set_recursion_layer_node_vk(self.load_recursive_layer_verification_key(
                ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
            )?)
            .unwrap();
        Ok(data_source)
    }

    pub fn save_keys_from_data_source(&self, source: &dyn SetupDataSource) -> anyhow::Result<()> {
        for base_circuit_type in
            (BaseLayerCircuitType::VM as u8)..=(BaseLayerCircuitType::L1MessagesHasher as u8)
        {
            let vk = source.get_base_layer_vk(base_circuit_type).map_err(|err| {
                anyhow::anyhow!("No vk exist for circuit type: {base_circuit_type}: {err}")
            })?;
            self.save_base_layer_verification_key(vk)
                .context("save_base_layer_vk()")?;

            let hint = source
                .get_base_layer_finalization_hint(base_circuit_type)
                .map_err(|err| {
                    anyhow::anyhow!(
                        "No finalization_hint exist for circuit type: {base_circuit_type}: {err}"
                    )
                })?
                .into_inner();
            let key = ProverServiceDataKey::new(base_circuit_type, AggregationRound::BasicCircuits);
            self.save_finalization_hints(key, &hint)
                .context("save_finalization_hints()")?;
        }
        for leaf_circuit_type in (ZkSyncRecursionLayerStorageType::LeafLayerCircuitForMainVM as u8)
            ..=(ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8)
        {
            let vk = source
                .get_recursion_layer_vk(leaf_circuit_type)
                .map_err(|err| {
                    anyhow::anyhow!("No vk exist for circuit type: {leaf_circuit_type}: {err}")
                })?;
            self.save_recursive_layer_verification_key(vk)
                .context("save_recursive_layer_vk()")?;

            let hint = source
                .get_recursion_layer_finalization_hint(leaf_circuit_type)
                .map_err(|err| {
                    anyhow::anyhow!(
                        "No finalization hint exist for circuit type: {leaf_circuit_type}: {err}"
                    )
                })?
                .into_inner();
            let key = ProverServiceDataKey::new_recursive(leaf_circuit_type);
            self.save_finalization_hints(key, &hint)
                .context("save_finalization_hints()")?;
        }
        self.save_recursive_layer_verification_key(
            source
                .get_recursion_layer_node_vk()
                .map_err(|err| anyhow::anyhow!("No vk exist for node layer circuit: {err}"))?,
        )
        .context("save_recursive_layer_vk")?;

        let node_hint = source
            .get_recursion_layer_node_finalization_hint()
            .map_err(|err| {
                anyhow::anyhow!("No finalization hint exist for node layer circuit: {err}")
            })?
            .into_inner();
        self.save_finalization_hints(
            ProverServiceDataKey::new_recursive(
                ZkSyncRecursionLayerStorageType::NodeLayerCircuit as u8,
            ),
            &node_hint,
        )
        .context("save_finalization_hints()")?;

        self.save_recursive_layer_verification_key(
            source
                .get_recursion_layer_vk(ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8)
                .map_err(|err| anyhow::anyhow!("No vk exist for scheduler circuit: {err}"))?,
        )
        .context("save_recursive_layer_vk")?;

        let scheduler_hint = source
            .get_recursion_layer_finalization_hint(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            )
            .map_err(|err| {
                anyhow::anyhow!("No finalization hint exist for scheduler layer circuit: {err}")
            })?
            .into_inner();

        self.save_finalization_hints(
            ProverServiceDataKey::new_recursive(
                ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8,
            ),
            &scheduler_hint,
        )
        .context("save_finalization_hints()")?;

        Ok(())
    }
}
