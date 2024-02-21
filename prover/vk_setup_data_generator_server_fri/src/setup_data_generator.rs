//! Set of functions to handle the generation of setup keys.
//! We generate separate set of keys for CPU and for GPU proving.

use std::collections::HashMap;

use anyhow::Context as _;
use circuit_definitions::{
    circuit_definitions::recursion_layer::ZkSyncRecursionLayerStorageType,
    zkevm_circuits::scheduler::aux::BaseLayerCircuitType,
};
use zkevm_test_harness::{
    compute_setups::{generate_circuit_setup_data, CircuitSetupData},
    data_source::SetupDataSource,
};
use zksync_prover_fri_types::ProverServiceDataKey;
use zksync_types::basic_fri_types::AggregationRound;
#[cfg(feature = "gpu")]
use {
    crate::GpuProverSetupData, shivini::cs::setup::GpuSetup, shivini::ProverContext,
    zksync_prover_fri_types::circuit_definitions::boojum::worker::Worker,
};

use crate::{keystore::Keystore, GoldilocksProverSetupData};

/// Internal helper function, that calls the test harness to generate the setup data.
/// It also does a final sanity check to make sure that verification keys didn't change.
pub fn generate_setup_data_common(
    keystore: &Keystore,
    is_base_layer: bool,
    circuit_type: u8,
) -> anyhow::Result<CircuitSetupData> {
    let mut data_source = keystore.load_keys_to_data_source()?;
    let circuit_setup_data =
        generate_circuit_setup_data(is_base_layer, circuit_type, &mut data_source).unwrap();

    let (finalization, vk) = if is_base_layer {
        (
            keystore.load_finalization_hints(ProverServiceDataKey::new(
                circuit_type,
                AggregationRound::BasicCircuits,
            ))?,
            data_source
                .get_base_layer_vk(circuit_type)
                .unwrap()
                .into_inner(),
        )
    } else {
        (
            keystore.load_finalization_hints(ProverServiceDataKey::new_recursive(circuit_type))?,
            data_source
                .get_recursion_layer_vk(circuit_type)
                .unwrap()
                .into_inner(),
        )
    };

    // Sanity check to make sure that generated setup data is matching.
    if finalization != circuit_setup_data.finalization_hint {
        anyhow::bail!("finalization hint mismatch for circuit: {circuit_type}");
    }
    if vk != circuit_setup_data.vk {
        anyhow::bail!("vk mismatch for circuit: {circuit_type}");
    }
    Ok(circuit_setup_data)
}

/// Trait to cover GPU and CPU setup data generation.
pub trait SetupDataGenerator {
    /// Generates the setup keys.
    fn generate_setup_data(
        &self,
        is_base_layer: bool,
        numeric_circuit: u8,
    ) -> anyhow::Result<Vec<u8>>;

    fn keystore(&self) -> &Keystore;

    /// Generates and stores the setup keys.
    /// Returns the md5 check sum of the stored file.
    fn generate_and_write_setup_data(
        &self,
        is_base_layer: bool,
        numeric_circuit: u8,
        dry_run: bool,
    ) -> anyhow::Result<String> {
        let serialized = self.generate_setup_data(is_base_layer, numeric_circuit)?;
        let digest = md5::compute(&serialized);

        if !dry_run {
            self.keystore()
                .save_setup_data_for_circuit_type(
                    if is_base_layer {
                        ProverServiceDataKey::new(numeric_circuit, AggregationRound::BasicCircuits)
                    } else {
                        ProverServiceDataKey::new_recursive(numeric_circuit)
                    },
                    &serialized,
                )
                .context("save_setup_data()")?;
        } else {
            tracing::warn!("Dry run - not writing the key");
        }
        Ok(format!("{:?}", digest))
    }

    fn generate_all(&self, dry_run: bool) -> anyhow::Result<HashMap<String, String>> {
        let mut result = HashMap::new();

        for numeric_circuit in
            BaseLayerCircuitType::VM as u8..=BaseLayerCircuitType::L1MessagesHasher as u8
        {
            let digest = self
                .generate_and_write_setup_data(true, numeric_circuit, dry_run)
                .context(format!("base layer, circuit {:?}", numeric_circuit))?;
            result.insert(format!("base_{}", numeric_circuit), digest);
        }

        for numeric_circuit in ZkSyncRecursionLayerStorageType::SchedulerCircuit as u8
            ..=ZkSyncRecursionLayerStorageType::LeafLayerCircuitForL1MessagesHasher as u8
        {
            let digest = self
                .generate_and_write_setup_data(false, numeric_circuit, dry_run)
                .context(format!("recursive layer, circuit {:?}", numeric_circuit))?;
            result.insert(format!("recursive_{}", numeric_circuit), digest);
        }
        Ok(result)
    }
}

pub struct CPUSetupDataGenerator {
    pub keystore: Keystore,
}

impl SetupDataGenerator for CPUSetupDataGenerator {
    fn generate_setup_data(
        &self,
        is_base_layer: bool,
        numeric_circuit: u8,
    ) -> anyhow::Result<Vec<u8>> {
        let circuit_setup_data =
            generate_setup_data_common(&self.keystore, is_base_layer, numeric_circuit)?;

        let prover_setup_data: GoldilocksProverSetupData = circuit_setup_data.into();

        Ok(bincode::serialize(&prover_setup_data).expect("Failed serializing setup data"))
    }

    fn keystore(&self) -> &Keystore {
        &self.keystore
    }
}

pub struct GPUSetupDataGenerator {
    pub keystore: Keystore,
}

impl SetupDataGenerator for GPUSetupDataGenerator {
    fn generate_setup_data(
        &self,
        is_base_layer: bool,
        numeric_circuit: u8,
    ) -> anyhow::Result<Vec<u8>> {
        #[cfg(not(feature = "gpu"))]
        {
            let _ = (is_base_layer, numeric_circuit);
            anyhow::bail!("Must compile with --gpu feature to use this option.");
        }
        #[cfg(feature = "gpu")]
        {
            let _context =
                ProverContext::create().context("failed initializing gpu prover context")?;
            let circuit_setup_data =
                generate_setup_data_common(&self.keystore, is_base_layer, numeric_circuit)?;

            let worker = Worker::new();
            let gpu_setup_data = GpuSetup::from_setup_and_hints(
                circuit_setup_data.setup_base,
                circuit_setup_data.setup_tree,
                circuit_setup_data.vars_hint.clone(),
                circuit_setup_data.wits_hint,
                &worker,
            )
            .context("failed creating GPU base layer setup data")?;
            let gpu_prover_setup_data = GpuProverSetupData {
                setup: gpu_setup_data,
                vk: circuit_setup_data.vk,
                finalization_hint: circuit_setup_data.finalization_hint,
            };
            // Serialization should always succeed.
            Ok(bincode::serialize(&gpu_prover_setup_data).expect("Failed serializing setup data"))
        }
    }

    fn keystore(&self) -> &Keystore {
        &self.keystore
    }
}
