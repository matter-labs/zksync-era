use anyhow::Context;
use common::{check_prover_prequisites, cmd::Cmd, logger, spinner::Spinner};
use config::{traits::SaveConfigWithBasePath, EcosystemConfig};
use xshell::{cmd, Shell};

use super::args::init_bellman_cuda::InitBellmanCudaArgs;
use crate::{
    consts::BELLMAN_CUDA_DIR,
    messages::{
        MSG_BELLMAN_CUDA_DIR_ERR, MSG_BELLMAN_CUDA_INITIALIZED, MSG_BUILDING_BELLMAN_CUDA_SPINNER,
        MSG_CLONING_BELLMAN_CUDA_SPINNER,
    },
};

pub(crate) async fn run(shell: &Shell, args: InitBellmanCudaArgs) -> anyhow::Result<()> {
    check_prover_prequisites(shell);

    let mut ecosystem_config = EcosystemConfig::from_file(shell)?;

    let args = args.fill_values_with_prompt()?;

    let bellman_cuda_dir = args.bellman_cuda_dir.unwrap_or("".to_string());
    let bellman_cuda_dir = if bellman_cuda_dir.is_empty() {
        clone_bellman_cuda(shell)?
    } else {
        bellman_cuda_dir
    };

    ecosystem_config.bellman_cuda_dir = Some(bellman_cuda_dir.clone().into());

    build_bellman_cuda(shell, &bellman_cuda_dir)?;

    ecosystem_config.save_with_base_path(shell, ".")?;

    logger::outro(MSG_BELLMAN_CUDA_INITIALIZED);
    Ok(())
}

fn clone_bellman_cuda(shell: &Shell) -> anyhow::Result<String> {
    let spinner = Spinner::new(MSG_CLONING_BELLMAN_CUDA_SPINNER);
    Cmd::new(cmd!(
        shell,
        "git clone https://github.com/matter-labs/era-bellman-cuda"
    ))
    .run()?;
    spinner.finish();

    Ok(shell
        .current_dir()
        .join(BELLMAN_CUDA_DIR)
        .to_str()
        .context(MSG_BELLMAN_CUDA_DIR_ERR)?
        .to_string())
}

fn build_bellman_cuda(shell: &Shell, bellman_cuda_dir: &str) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_BUILDING_BELLMAN_CUDA_SPINNER);
    Cmd::new(cmd!(
        shell,
        "cmake -B{bellman_cuda_dir}/build -S{bellman_cuda_dir}/ -DCMAKE_BUILD_TYPE=Release"
    ))
    .run()?;
    Cmd::new(cmd!(shell, "cmake --build {bellman_cuda_dir}/build")).run()?;
    spinner.finish();
    Ok(())
}
