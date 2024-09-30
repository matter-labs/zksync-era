use std::{
    fs::File,
    io::{Read, Write},
};

use anyhow::{bail, Context, Result};
use tracing_log::LogTracer;
use tracing_subscriber::{fmt, prelude::*, EnvFilter, Registry};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_prover_interface::inputs::{TeeVerifierInput, V1TeeVerifierInput, WitnessInputData};
use zksync_tee_verifier::Verify;
use zksync_types::{
    get_system_context_key, url::SensitiveUrl, L1BatchNumber, L2ChainId,
    SYSTEM_CONTEXT_CHAIN_ID_POSITION,
};
use zksync_utils::h256_to_u32;
use zksync_vm_executor::storage::L1BatchParamsProvider;

#[tokio::main]
async fn main() -> Result<()> {
    LogTracer::init().context("Failed to set logger")?;

    let subscriber = Registry::default()
        .with(EnvFilter::from_default_env())
        .with(fmt::layer().with_writer(std::io::stderr));
    tracing::subscriber::set_global_default(subscriber).context("Failed to set logger")?;
    //let l1_batch_number = L1BatchNumber(10530);
    let l1_batch_number = L1BatchNumber(492397);
    produce_input(l1_batch_number).await?;

    let tee_verifier_input: TeeVerifierInput = {
        use std::{fs::File, io::Read};
        let filename =
            format!("tee_verifier_inputs_tee_verifier_input_for_l1_batch_{l1_batch_number}.bin");

        let mut file = File::open(&filename).context(format!("failed to open {filename}"))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .context("failed to read tee_verifier_input.bin")?;
        bincode::deserialize(&buffer).context("failed to deserialize binary contents")?
    };

    match tee_verifier_input {
        TeeVerifierInput::V1(tvi) => {
            tvi.verify()?;
        }
        _ => bail!("error!"),
    }

    Ok(())
}

async fn produce_input(l1_batch_number: L1BatchNumber) -> Result<()> {
    let username = std::env::var("PGUSER")?;
    let password = std::env::var("PGPASSWORD")?;
    let hostname = std::env::var("PGHOSTNAME")?;
    let dbname = std::env::var("PGDBNAME")?;
    let url: SensitiveUrl =
        format!("postgres://{username}:{password}@{hostname}/{dbname}").parse()?;

    let connection_pool = ConnectionPool::<Core>::builder(url, 3).build().await?;

    let witness_inputs: WitnessInputData = {
        let filename = format!("witness_inputs_{l1_batch_number}.bin");

        let mut file = File::open(&filename).context(format!("failed to open {filename}"))?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)
            .context("failed to read witness_inputs.bin")?;
        bincode::deserialize(&buffer).context("failed to deserialize binary contents")?
    };

    let chain_id_key = get_system_context_key(SYSTEM_CONTEXT_CHAIN_ID_POSITION);
    let l2_chain_id = L2ChainId::from(h256_to_u32(
        witness_inputs
            .vm_run_data
            .witness_block_state
            .read_storage_key
            .get(&chain_id_key)
            .context("Failed to get L2ChainId from witness inputs")?
            .clone(),
    ));

    let mut connection = connection_pool
        .connection()
        .await
        .context("failed to get connection for TeeVerifierInputProducer")?;

    let l2_blocks_execution_data = connection
        .transactions_dal()
        .get_l2_blocks_to_execute_for_l1_batch(l1_batch_number)
        .await?;

    let l1_batch_params_provider = L1BatchParamsProvider::new(&mut connection)
        .await
        .context("failed initializing L1 batch params provider")?;

    // In the state keeper, this value is used to reject execution.
    // All batches have already been executed by State Keeper.
    // This means we don't want to reject any execution, therefore we're using MAX as an allow all.
    let validation_computational_gas_limit = u32::MAX;

    let (system_env, l1_batch_env) = l1_batch_params_provider
        .load_l1_batch_env(
            &mut connection,
            l1_batch_number,
            validation_computational_gas_limit,
            l2_chain_id,
        )
        .await?
        .with_context(|| format!("expected L1 batch #{l1_batch_number} to be sealed"))?;

    tracing::info!("Started execution of l1_batch: {l1_batch_number:?}");

    let tee_verifier_input = V1TeeVerifierInput::new(
        witness_inputs.vm_run_data,
        witness_inputs.merkle_paths,
        l2_blocks_execution_data,
        l1_batch_env,
        system_env,
    );

    let tee_verifier_input = TeeVerifierInput::new(tee_verifier_input);

    // Serialize and save l2_blocks_execution_data to a file
    let serialized_data = bincode::serialize(&tee_verifier_input)?;
    let filename =
        format!("tee_verifier_inputs_tee_verifier_input_for_l1_batch_{l1_batch_number}.bin");
    let mut file = File::create(&filename).context(format!("failed to create {filename}"))?;
    file.write_all(&serialized_data)
        .context("failed to write tee_verifier_input to file")?;

    Ok(())
}
