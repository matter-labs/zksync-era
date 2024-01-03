import { Command } from 'commander';
import * as utils from './utils';

const IMAGES = [
    'server-v2',
    'external-node',
    'cross-external-nodes-checker',
    'contract-verifier',
    'prover-v2',
    'geth',
    'local-node',
    'zk-environment',
    'circuit-synthesizer',
    'witness-generator',
    'prover-fri',
    'prover-gpu-fri',
    'witness-vector-generator',
    'prover-fri-gateway',
    'proof-fri-compressor',
    'snapshots-creator'
];

const DOCKER_REGISTRIES = ['us-docker.pkg.dev/matterlabs-infra/matterlabs-docker', 'matterlabs'];

const UNIX_TIMESTAMP = Date.now();

async function dockerCommand(
    command: 'push' | 'build',
    image: string,
    platforms: string[] = ['linux/amd64'],
    customTag?: string,
    buildExtraArgs: string = '',
    dockerOrg: string = 'matterlabs'
) {
    // Generating all tags for containers. We need 2 tags here: SHA and SHA+TS
    const { stdout: COMMIT_SHORT_SHA }: { stdout: string } = await utils.exec('git rev-parse --short HEAD');
    // COMMIT_SHORT_SHA returns with newline, so we need to trim it
    const imageTagShaTS: string = process.env.IMAGE_TAG_SUFFIX
        ? process.env.IMAGE_TAG_SUFFIX
        : `${COMMIT_SHORT_SHA.trim()}-${UNIX_TIMESTAMP}`;

    // We want an alternative flow for Rust image
    if (image == 'rust') {
        await dockerCommand(command, 'server-v2', platforms, customTag, dockerOrg);
        await dockerCommand(command, 'prover', platforms, customTag, dockerOrg);
        return;
    }
    if (!IMAGES.includes(image)) {
        throw new Error(`Wrong image name: ${image}`);
    }

    if (image == 'keybase') {
        image = 'keybase-secret';
    }

    const tagList = customTag ? [customTag] : defaultTagList(image, COMMIT_SHORT_SHA.trim(), imageTagShaTS);

    // Main build\push flow
    switch (command) {
        case 'build':
            await _build(image, tagList, dockerOrg, platforms, buildExtraArgs);
            break;
        default:
            console.log(`Unknown command for docker ${command}.`);
            break;
    }
}

function defaultTagList(image: string, imageTagSha: string, imageTagShaTS: string) {
    const tagList = [
        'server-v2',
        'external-node',
        'cross-external-nodes-checker',
        'prover',
        'contract-verifier',
        'prover-v2',
        'circuit-synthesizer',
        'witness-generator',
        'prover-fri',
        'prover-gpu-fri',
        'witness-vector-generator',
        'prover-fri-gateway',
        'proof-fri-compressor',
        'snapshots-creator'
    ].includes(image)
        ? ['latest', 'latest2.0', `2.0-${imageTagSha}`, `${imageTagSha}`, `2.0-${imageTagShaTS}`, `${imageTagShaTS}`]
        : [`latest2.0`, 'latest'];

    return tagList;
}

async function _build(
    image: string,
    tagList: string[],
    dockerOrg: string,
    platforms: string[],
    extraArgs: string = ''
) {
    let tagsToBuild = '';

    for (const tag of tagList) {
        for (const registry of DOCKER_REGISTRIES) {
            tagsToBuild = tagsToBuild + `-t ${registry}/${image}:${tag} `;
        }
    }

    let buildArgs = '';
    if (image === 'prover-v2') {
        const eraBellmanCudaRelease = process.env.ERA_BELLMAN_CUDA_RELEASE;
        buildArgs += `--build-arg ERA_BELLMAN_CUDA_RELEASE=${eraBellmanCudaRelease}`;
    }
    if (image === 'prover-gpu-fri') {
        const cudaArch = process.env.CUDA_ARCH;
        buildArgs += `--build-arg CUDA_ARCH='${cudaArch}' `;
    }
    buildArgs += extraArgs;

    const imagePath = image === 'prover-v2' ? 'prover' : image;

    const buildCommand =
        `DOCKER_BUILDKIT=1 docker buildx build ${tagsToBuild}` +
        ` --platform=${platforms.join(',')}` +
        (buildArgs ? ` ${buildArgs}` : '') +
        ` -f ./docker/${imagePath}/Dockerfile .`;

    await utils.spawn(buildCommand);
}

export async function build(image: string, cmd: Command) {
    await dockerCommand('build', image, cmd.platforms, cmd.customTag);
}

export async function customBuildForHyperchain(image: string, dockerOrg: string) {
    await dockerCommand('build', image, ['linux/amd64'], dockerOrg);
}

export async function push(image: string, cmd: Command) {
    await dockerCommand('build', image, cmd.platforms, cmd.customTag, '--push');
}

export async function restart(container: string) {
    await utils.spawn(`docker compose restart ${container}`);
}

export async function pull() {
    await utils.spawn('docker compose pull');
}

export const command = new Command('docker').description('docker management');

command
    .command('build <image>')
    .option('--custom-tag <value>', 'Custom tag for image')
    .option('--platforms <platforms>', 'Comma-separated list of platforms', (val) => val.split(','), ['linux/amd64'])
    .description('build docker image')
    .action(build);
command
    .command('push <image>')
    .option('--custom-tag <value>', 'Custom tag for image')
    .option('--platforms <platforms>', 'Comma-separated list of platforms', (val) => val.split(','), ['linux/amd64'])
    .action(push);
command.command('pull').description('pull all containers').action(pull);
command.command('restart <container>').description('restart container in docker-compose.yml').action(restart);
