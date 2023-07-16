import * as hre from 'hardhat';
import * as fs from 'fs';
import { spawn as _spawn } from 'child_process';

import { getZksolcPath, getZksolcUrl, saltFromUrl } from '@matterlabs/hardhat-zksync-solc';

const COMPILER_VERSION = '1.3.13';
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

export async function compileYul(path: string, files: string[], outputDirName: string | null) {
    if (!files.length) {
        console.log(`No test files provided in folder ${path}.`);
        return;
    }
    let paths = preparePaths(path, files, outputDirName);

    const zksolcLocation = await compilerLocation();
    await spawn(
        `${zksolcLocation} ${paths.absolutePathSources}/${paths.outputDir} --optimization 3 --system-mode --yul --bin --overwrite -o ${paths.absolutePathArtifacts}/${paths.outputDir}`
    );
}

export async function compileYulFolder(path: string) {
    let files: string[] = (await fs.promises.readdir(path)).filter((fn) => fn.endsWith('.yul'));
    for (const file of files) {
        await compileYul(path, [file], `${file}`);
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
    await compileYulFolder('contracts/yul');
}

main()
    .then(() => process.exit(0))
    .catch((err) => {
        console.error('Error:', err.message || err);
        process.exit(1);
    });
