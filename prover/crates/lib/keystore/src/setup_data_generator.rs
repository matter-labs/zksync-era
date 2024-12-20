//! Set of functions to handle the generation of setup keys.
//! We generate separate set of keys for CPU and for GPU proving.

use std::collections::HashMap;

use anyhow::Context as _;
use zkevm_test_harness::{
    compute_setups::{
        generate_circuit_setup_data, light::generate_light_circuit_setup_data, CircuitSetupData,
    },
    data_source::SetupDataSource,
};
use zksync_prover_fri_types::ProverServiceDataKey;
#[cfg(feature = "gpu")]
use {
    crate::GpuProverSetupData,
    boojum_cuda::poseidon2::GLHasher,
    shivini::{cs::gpu_setup_and_vk_from_base_setup_vk_params_and_hints, ProverContext},
    zksync_prover_fri_types::circuit_definitions::boojum::worker::Worker,
};

use crate::{keystore::Keystore, GoldilocksProverSetupData};

/// Internal helper function, that calls the test harness to generate the setup data.
/// It also does a final sanity check to make sure that verification keys didn't change.
pub fn generate_setup_data_common(
    keystore: &Keystore,
    circuit: ProverServiceDataKey,
) -> anyhow::Result<CircuitSetupData> {
    let mut data_source = keystore.load_keys_to_data_source()?;
    let circuit_setup_data = generate_circuit_setup_data(
        circuit.round as u8, // TODO: Actually it's called "ProvingStage" now
        circuit.circuit_id,
        &mut data_source,
    )
    .unwrap();

    let (finalization, vk) = if circuit.is_base_layer() {
        (
            Some(keystore.load_finalization_hints(circuit)?),
            data_source
                .get_base_layer_vk(circuit.circuit_id)
                .unwrap()
                .into_inner(),
        )
    } else {
        (
            Some(keystore.load_finalization_hints(circuit)?),
            data_source
                .get_recursion_layer_vk(circuit.circuit_id)
                .unwrap()
                .into_inner(),
        )
    };

    // Sanity check to make sure that generated setup data is matching.
    if let Some(finalization) = finalization {
        if finalization != circuit_setup_data.finalization_hint {
            anyhow::bail!(
                "finalization hint mismatch for circuit: {:?}",
                circuit.name()
            );
        }
    }
    if vk != circuit_setup_data.vk {
        anyhow::bail!("vk mismatch for circuit: {:?}", circuit.name());
    }
    Ok(circuit_setup_data)
}

/// Trait to cover GPU and CPU setup data generation.
pub trait SetupDataGenerator {
    /// Generates the setup keys.
    fn generate_setup_data(&self, circuit: ProverServiceDataKey) -> anyhow::Result<Vec<u8>>;

    fn keystore(&self) -> &Keystore;

    /// Generates and stores the setup keys.
    /// Returns the md5 check sum of the stored file.
    fn generate_and_write_setup_data(
        &self,
        circuit: ProverServiceDataKey,
        dry_run: bool,
        recompute_if_missing: bool,
    ) -> anyhow::Result<String> {
        if recompute_if_missing && self.keystore().is_setup_data_present(&circuit) {
            tracing::info!(
                "Skipping setup computation for {:?} as file is already present.",
                circuit.name(),
            );
            return Ok("Skipped".to_string());
        }
        let serialized = self.generate_setup_data(circuit)?;
        let digest = md5::compute(&serialized);

        if !dry_run {
            self.keystore()
                .save_setup_data_for_circuit_type(circuit, &serialized)
                .context("save_setup_data()")?;
        } else {
            tracing::warn!("Dry run - not writing the key");
        }
        Ok(format!("{:?}", digest))
    }

    /// Generate all setup keys for boojum circuits.
    fn generate_all(
        &self,
        dry_run: bool,
        recompute_if_missing: bool,
    ) -> anyhow::Result<HashMap<String, String>> {
        Ok(ProverServiceDataKey::all_boojum()
            .iter()
            .map(|circuit| {
                let digest = self
                    .generate_and_write_setup_data(*circuit, dry_run, recompute_if_missing)
                    .context(circuit.name())
                    .unwrap();
                (circuit.name(), digest)
            })
            .collect())
    }
}

pub struct CPUSetupDataGenerator {
    pub keystore: Keystore,
}

impl SetupDataGenerator for CPUSetupDataGenerator {
    fn generate_setup_data(&self, circuit: ProverServiceDataKey) -> anyhow::Result<Vec<u8>> {
        let circuit_setup_data = generate_setup_data_common(&self.keystore, circuit)?;

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
    fn generate_setup_data(&self, circuit: ProverServiceDataKey) -> anyhow::Result<Vec<u8>> {
        #[cfg(not(feature = "gpu"))]
        {
            let _ = circuit;
            anyhow::bail!("Must compile with --gpu feature to use this option.");
        }
        #[cfg(feature = "gpu")]
        {
            let _context =
                ProverContext::create().context("failed initializing gpu prover context")?;

            let mut data_source = self.keystore.load_keys_to_data_source()?;
            let circuit_setup_data = generate_light_circuit_setup_data(
                circuit.round as u8,
                circuit.circuit_id,
                &mut data_source,
            )
            .unwrap();

            let worker = Worker::new();
            // TODO: add required assertions
            let (gpu_setup_data, vk) =
                gpu_setup_and_vk_from_base_setup_vk_params_and_hints::<GLHasher, _>(
                    circuit_setup_data.setup_base,
                    circuit_setup_data.vk_geometry,
                    circuit_setup_data.vars_hint.clone(),
                    circuit_setup_data.wits_hint,
                    &worker,
                )
                .context("failed creating GPU base layer setup data")?;
            let gpu_prover_setup_data = GpuProverSetupData {
                setup: gpu_setup_data,
                vk: vk.clone(),
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
