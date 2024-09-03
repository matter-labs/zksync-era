use xshell::{cmd, Shell};

use crate::{cmd::Cmd, logger};

fn prerequisites() -> [Prerequisite; 5] {
    [
        Prerequisite {
            name: "git",
            download_link: "https://git-scm.com/book/en/v2/Getting-Started-Installing-Git",
            custom_validator: None,
        },
        Prerequisite {
            name: "docker",
            download_link: "https://docs.docker.com/get-docker/",
            custom_validator: None,
        },
        Prerequisite {
            name: "forge",
            download_link:
                "https://github.com/matter-labs/foundry-zksync?tab=readme-ov-file#quick-install",
            custom_validator: Some(Box::new(|| {
                let shell = Shell::new().unwrap();
                let Ok(result) = Cmd::new(cmd!(shell, "forge build --help")).run_with_output()
                else {
                    return false;
                };
                let Ok(stdout) = String::from_utf8(result.stdout) else {
                    return false;
                };
                stdout.contains("ZKSync configuration")
            })),
        },
        Prerequisite {
            name: "cargo",
            download_link: "https://doc.rust-lang.org/cargo/getting-started/installation.html",
            custom_validator: None,
        },
        Prerequisite {
            name: "yarn",
            download_link: "https://yarnpkg.com/getting-started/install",
            custom_validator: None,
        },
    ]
}

const DOCKER_COMPOSE_PREREQUISITE: Prerequisite = Prerequisite {
    name: "docker compose",
    download_link: "https://docs.docker.com/compose/install/",
    custom_validator: None,
};

const PROVER_PREREQUISITES: [Prerequisite; 5] = [
    Prerequisite {
        name: "gcloud",
        download_link: "https://cloud.google.com/sdk/docs/install",
        custom_validator: None,
    },
    Prerequisite {
        name: "wget",
        download_link: "https://www.gnu.org/software/wget/",
        custom_validator: None,
    },
    Prerequisite {
        name: "cmake",
        download_link: "https://cmake.org/download/",
        custom_validator: None,
    },
    Prerequisite {
        name: "nvcc",
        download_link: "https://developer.nvidia.com/cuda-downloads",
        custom_validator: None,
    }, // CUDA toolkit
    Prerequisite {
        name: "nvidia-smi",
        download_link: "https://developer.nvidia.com/cuda-downloads",
        custom_validator: None,
    }, // CUDA GPU driver
];

struct Prerequisite {
    name: &'static str,
    download_link: &'static str,
    custom_validator: Option<Box<dyn Fn() -> bool>>,
}

pub fn check_general_prerequisites(shell: &Shell) {
    check_prerequisites(shell, &prerequisites(), true);
}

pub fn check_prover_prequisites(shell: &Shell) {
    check_prerequisites(shell, &PROVER_PREREQUISITES, false);
}

fn check_prerequisites(shell: &Shell, prerequisites: &[Prerequisite], check_compose: bool) {
    let mut missing_prerequisites = vec![];

    for prerequisite in prerequisites {
        if !check_prerequisite(shell, prerequisite) {
            missing_prerequisites.push(prerequisite);
        }
    }

    if check_compose && !check_docker_compose_prerequisite(shell) {
        missing_prerequisites.push(&DOCKER_COMPOSE_PREREQUISITE);
    }

    if !missing_prerequisites.is_empty() {
        logger::error("Prerequisite check has failed");
        logger::error_note(
            "The following prerequisites are missing",
            &missing_prerequisites
                .iter()
                .map(|prerequisite| {
                    format!("- {} ({})", prerequisite.name, prerequisite.download_link)
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
        logger::outro("Failed");
        std::process::exit(1);
    }
}

fn check_prerequisite(shell: &Shell, prerequisite: &Prerequisite) -> bool {
    let name = prerequisite.name;
    if Cmd::new(cmd!(shell, "which {name}")).run().is_err() {
        return false;
    }
    let Some(custom) = &prerequisite.custom_validator else {
        return true;
    };
    custom()
}

fn check_docker_compose_prerequisite(shell: &Shell) -> bool {
    Cmd::new(cmd!(shell, "docker compose version"))
        .run()
        .is_ok()
}
