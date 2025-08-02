use anyhow::Context;
use xshell::{cmd, Shell};
use zkstack_cli_common::{
    check_prerequisites, cmd::Cmd, git, logger, spinner::Spinner, GPU_PREREQUISITES,
};
use zkstack_cli_config::{traits::SaveConfigWithBasePath, ZkStackConfig};

use super::args::init_bellman_cuda::InitBellmanCudaArgs;
use crate::{
    consts::BELLMAN_CUDA_DIR,
    messages::{
        MSG_BELLMAN_CUDA_DIR_ERR, MSG_BELLMAN_CUDA_INITIALIZED, MSG_BUILDING_BELLMAN_CUDA_SPINNER,
        MSG_CLONING_BELLMAN_CUDA_SPINNER,
    },
};

pub(crate) async fn run(shell: &Shell, args: InitBellmanCudaArgs) -> anyhow::Result<()> {
    check_prerequisites(shell, &GPU_PREREQUISITES, false);

    let mut ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    let args = args.fill_values_with_prompt();

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
    let path = shell.current_dir().join(BELLMAN_CUDA_DIR);
    if shell.path_exists(path.clone()) {
        return Ok(path.to_str().context(MSG_BELLMAN_CUDA_DIR_ERR)?.to_string());
    }

    let spinner = Spinner::new(MSG_CLONING_BELLMAN_CUDA_SPINNER);
    let path = git::clone(
        shell,
        shell.current_dir(),
        "https://github.com/matter-labs/era-bellman-cuda",
        BELLMAN_CUDA_DIR,
    )?;
    spinner.finish();

    Ok(path.to_str().context(MSG_BELLMAN_CUDA_DIR_ERR)?.to_string())
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
