import * as hre from 'hardhat';
import * as fs from 'fs';
import { spawn as _spawn } from 'child_process';

import { getZksolcPath, getZksolcUrl, saltFromUrl } from '@matterlabs/hardhat-zksync-solc';

const COMPILER_VERSION = '1.3.14';
const IS_COMPILER_PRE_RELEASE = false;

async function compilerLocation(): Promise<string> {
    if (IS_COMPILER_PRE_RELEASE) {
        const url = getZksolcUrl('https://github.com/matter-labs/zksolc-prerelease', hre.config.zksolc.version);
        const salt = saltFromUrl(url);
        return await getZksolcPath(COMPILER_VERSION, salt);
    } else {
        return await getZksolcPath(COMPILER_VERSION, '');
    }
}

// executes a command in a new shell
// but pipes data to parent's stdout/stderr
export function spawn(command: string) {
    command = command.replace(/\n/g, ' ');
    const child = _spawn(command, { stdio: 'inherit', shell: true });
    return new Promise((resolve, reject) => {
        child.on('error', reject);
        child.on('close', (code) => {
            code == 0 ? resolve(code) : reject(`Child process exited with code ${code}`);
        });
    });
}

export async function compile(path: string, files: string[], outputDirName: string | null, type: string) {
    if (!files.length) {
        console.log(`No test files provided in folder ${path}.`);
        return;
    }
    let paths = preparePaths(path, files, outputDirName);

    let systemMode = type === 'yul' ? '--system-mode  --optimization 3' : '';

    const zksolcLocation = await compilerLocation();
    await spawn(
        `${zksolcLocation} ${paths.absolutePathSources}/${paths.outputDir} ${systemMode} --${type} --bin --overwrite -o ${paths.absolutePathArtifacts}/${paths.outputDir}`
    );
}

export async function compileFolder(path: string, type: string) {
    let files: string[] = (await fs.promises.readdir(path)).filter((fn) => fn.endsWith(`.${type}`));
    for (const file of files) {
        await compile(path, [file], `${file}`, type);
    }
}

function preparePaths(path: string, files: string[], outputDirName: string | null): CompilerPaths {
    const filePaths = files
        .map((val, _) => {
            return `sources/${val}`;
        })
        .join(' ');
    const outputDir = outputDirName || files[0];
    let absolutePathSources = `${process.env.ZKSYNC_HOME}/core/tests/ts-integration/${path}`;

    let absolutePathArtifacts = `${process.env.ZKSYNC_HOME}/core/tests/ts-integration/${path}/artifacts`;

    return new CompilerPaths(filePaths, outputDir, absolutePathSources, absolutePathArtifacts);
}

class CompilerPaths {
    public filePath: string;
    public outputDir: string;
    public absolutePathSources: string;
    public absolutePathArtifacts: string;
    constructor(filePath: string, outputDir: string, absolutePathSources: string, absolutePathArtifacts: string) {
        this.filePath = filePath;
        this.outputDir = outputDir;
        this.absolutePathSources = absolutePathSources;
        this.absolutePathArtifacts = absolutePathArtifacts;
    }
}

async function main() {
    await compileFolder('contracts/yul', 'yul');
    await compileFolder('contracts/zkasm', 'zkasm');
}

main()
    .then(() => process.exit(0))
    .catch((err) => {
        console.error('Error:', err.message || err);
        process.exit(1);
    });
