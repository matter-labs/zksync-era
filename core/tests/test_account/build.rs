use std::{
    env, fs,
    path::Path,
    process::{Command, Output},
};

fn assert_output_success(output: &Output, cmd: &str) {
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "`{cmd}` failed with {status}\n---- stdout ---- \n{stdout}\n---- stderr ----\n{stderr}",
            status = output.status
        );
    }
}

fn check_yarn() {
    let output = Command::new("yarn")
        .arg("--version")
        .output()
        .expect("failed running `yarn --version`");
    assert_output_success(&output, "yarn --version");
    let yarn_version = output.stdout;
    let yarn_version = String::from_utf8(yarn_version).expect("yarn version is not UFT-8");
    assert!(
        yarn_version.starts_with("1."),
        "Unsupported yarn version: {yarn_version}"
    );
}

fn copy_recursively(src_dir: &Path, dest_dir: &Path) {
    fs::create_dir_all(dest_dir).unwrap_or_else(|err| {
        panic!(
            "failed creating destination dir `{}`: {err}",
            dest_dir.display()
        );
    });
    for entry in fs::read_dir(src_dir).expect("failed reading source dir") {
        let entry = entry.unwrap();
        let dest_path = dest_dir.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_recursively(&entry.path(), &dest_path);
        } else {
            fs::copy(entry.path(), &dest_path).unwrap_or_else(|err| {
                panic!(
                    "failed copying `{}` to `{}`: {err}",
                    entry.path().display(),
                    dest_path.display()
                );
            });
        }
    }
}

fn copy_contract_files(out_dir: &str) {
    const COPIED_FILES: &[&str] = &["package.json", "yarn.lock", "hardhat.config.ts"];
    const COPIED_DIRS: &[&str] = &["contracts"];

    let crate_dir = env::var("CARGO_MANIFEST_DIR").expect("no `CARGO_MANIFEST_DIR` provided");

    for &copied_file in COPIED_FILES {
        let src = Path::new(&crate_dir).join(copied_file);
        let dest = Path::new(&out_dir).join(copied_file);

        if fs::exists(&dest).unwrap_or(false) {
            fs::remove_file(&dest).unwrap_or_else(|err| {
                panic!("failed removing `{}`: {err}", dest.display());
            });
        }
        fs::copy(&src, &dest).unwrap_or_else(|err| {
            panic!("failed copying `{copied_file}`: {err}");
        });
    }
    for &copied_dir in COPIED_DIRS {
        let src = Path::new(&crate_dir).join(copied_dir);
        let dest = Path::new(&out_dir).join(copied_dir);

        if fs::exists(&dest).unwrap_or(false) {
            fs::remove_dir_all(&dest).unwrap_or_else(|err| {
                panic!("failed removing `{}`: {err}", dest.display());
            });
        }
        copy_recursively(&src, &dest);
    }
}

fn install_yarn_deps(working_dir: &str) {
    let output = Command::new("yarn")
        .current_dir(working_dir)
        .args(["--non-interactive", "install"])
        .output()
        .expect("failed running `yarn install`");
    assert_output_success(&output, "yarn install");
}

fn compile_contracts(working_dir: &str) {
    let output = Command::new("yarn")
        .current_dir(working_dir)
        .args(["--non-interactive", "run", "build"])
        .output()
        .expect("failed running `yarn run build`");
    assert_output_success(&output, "yarn run build");
}

fn main() {
    check_yarn();

    println!("cargo::rerun-if-changed=package.json");
    println!("cargo::rerun-if-changed=yarn.lock");
    println!("cargo::rerun-if-changed=hardhat.config.ts");
    println!("cargo::rerun-if-changed=contracts");

    let temp_dir = env::var("OUT_DIR").expect("no `OUT_DIR` provided");
    copy_contract_files(&temp_dir);
    install_yarn_deps(&temp_dir);
    compile_contracts(&temp_dir);
}
