use common::{check_prover_prequisites, cmd::Cmd, logger, spinner::Spinner};
use config::{traits::SaveConfigWithBasePath, EcosystemConfig};
use xshell::{cmd, Shell};

use super::args::init_bellman_cuda::InitBellmanCudaArgs;
use crate::messages::{
    MSG_BELLMAN_CUDA_DIR_ERR, MSG_BELLMAN_CUDA_INITIALIZED, MSG_BUILDING_BELLMAN_CUDA_SPINNER,
    MSG_CLONING_BELLMAN_CUDA_SPINNER,
};

pub(crate) async fn run(shell: &Shell, args: InitBellmanCudaArgs) -> anyhow::Result<()> {
    check_prover_prequisites(shell);

    let mut ecosystem_config = EcosystemConfig::from_file(shell)?;

    let args = args.fill_values_with_prompt(ecosystem_config.bellman_cuda_dir)?;

    let bellman_cuda_dir = args.bellman_cuda_dir.expect(MSG_BELLMAN_CUDA_DIR_ERR);
    ecosystem_config.bellman_cuda_dir = Some(bellman_cuda_dir.clone().into());

    if !shell.path_exists(&bellman_cuda_dir) {
        clone_bellman_cuda(shell, &bellman_cuda_dir)?;
    }

    build_bellman_cuda(shell, &bellman_cuda_dir)?;

    ecosystem_config.save_with_base_path(shell, ".")?;

    logger::outro(MSG_BELLMAN_CUDA_INITIALIZED);
    Ok(())
}

fn clone_bellman_cuda(shell: &Shell, bellman_cuda_dir: &str) -> anyhow::Result<()> {
    let spinner = Spinner::new(MSG_CLONING_BELLMAN_CUDA_SPINNER);
    Cmd::new(cmd!(
        shell,
        "git clone https://github.com/matter-labs/era-bellman-cuda {bellman_cuda_dir}"
    ))
    .run()?;
    spinner.finish();
    Ok(())
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
