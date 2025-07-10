import * as hre from "hardhat";
import * as fs from "fs";
// eslint-disable-next-line @typescript-eslint/no-unused-vars
import { exec as _exec, spawn as _spawn } from "child_process";

import { getZksolcUrl, saltFromUrl } from "@matterlabs/hardhat-zksync-solc";
import { getCompilersDir } from "hardhat/internal/util/global-dir";
import path from "path";

const COMPILER_VERSION = "1.5.10";
const IS_COMPILER_PRE_RELEASE = false;

async function compilerLocation(): Promise<string> {
  const compilersCache = await getCompilersDir();

  let salt = "";

  if (IS_COMPILER_PRE_RELEASE) {
    const url = getZksolcUrl(
      "https://github.com/matter-labs/zksolc-prerelease",
      hre.config.zksolc.version,
    );
    salt = saltFromUrl(url);
  }

  return path.join(
    compilersCache,
    "zksolc",
    `zksolc-v${COMPILER_VERSION}${salt ? "-" : ""}${salt}`,
  );
}

// but pipes data to parent's stdout/stderr
export function spawn(command: string) {
  command = command.replace(/\n/g, " ");
  const child = _spawn(command, { stdio: "inherit", shell: true });
  return new Promise((resolve, reject) => {
    child.on("error", reject);
    child.on("close", (code) => {
      code == 0
        ? resolve(code)
        : reject(`Child process exited with code ${code}`);
    });
  });
}

export async function compile(
  pathToHome: string,
  path: string,
  files: string[],
  outputDirName: string | null,
  type: string,
) {
  if (!files.length) {
    console.log(`No test files provided in folder ${path}.`);
    return;
  }
  let paths = preparePaths(pathToHome, path, files, outputDirName);

  let eraVmExtensions =
    type === "yul" ? "--enable-eravm-extensions  --optimization 3" : "";

  const zksolcLocation = await compilerLocation();
  await spawn(
    `${zksolcLocation} ${paths.absolutePathSources}/${paths.outputDir} ${eraVmExtensions} --${type} --bin --overwrite -o ${paths.absolutePathArtifacts}/${paths.outputDir}`,
  );
}

export async function compileFolder(
  pathToHome: string,
  path: string,
  type: string,
) {
  let compilationMode;
  if (type === "zkasm") {
    compilationMode = "eravm-assembly";
  } else {
    compilationMode = type;
  }
  let files: string[] = (await fs.promises.readdir(path)).filter((fn) =>
    fn.endsWith(`.${type}`),
  );
  for (const file of files) {
    await compile(pathToHome, path, [file], `${file}`, compilationMode);
  }
}

function preparePaths(
  pathToHome: string,
  path: string,
  files: string[],
  outputDirName: string | null,
): CompilerPaths {
  const filePaths = files
    .map((val, _) => {
      return `sources/${val}`;
    })
    .join(" ");
  const outputDir = outputDirName || files[0];
  let absolutePathSources = `${pathToHome}/core/tests/ts-integration/${path}`;

  let absolutePathArtifacts = `${pathToHome}/core/tests/ts-integration/${path}/artifacts`;

  return new CompilerPaths(
    filePaths,
    outputDir,
    absolutePathSources,
    absolutePathArtifacts,
  );
}

class CompilerPaths {
  public filePath: string;
  public outputDir: string;
  public absolutePathSources: string;
  public absolutePathArtifacts: string;

  constructor(
    filePath: string,
    outputDir: string,
    absolutePathSources: string,
    absolutePathArtifacts: string,
  ) {
    this.filePath = filePath;
    this.outputDir = outputDir;
    this.absolutePathSources = absolutePathSources;
    this.absolutePathArtifacts = absolutePathArtifacts;
  }
}

async function main() {
  const pathToHome = path.join(__dirname, "../../../../");
  await compileFolder(pathToHome, "contracts/yul", "yul");
  await compileFolder(pathToHome, "contracts/zkasm", "zkasm");
}

main()
  .then(() => process.exit(0))
  .catch((err) => {
    console.error("Error:", err.message || err);
    process.exit(1);
  });
