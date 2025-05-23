import { Command } from 'commander';
import * as utils from 'utils';

const IMAGES = [
    'server-v2',
    'external-node',
    'contract-verifier',
    'local-node',
    'zk-environment',
    'circuit-synthesizer',
    'witness-generator',
    'prover-gpu-fri',
    'witness-vector-generator',
    'circuit-prover-gpu',
    'prover-fri-gateway',
    'prover-job-monitor',
    'proof-fri-gpu-compressor',
    'snapshots-creator',
    'verified-sources-fetcher',
    'prover-autoscaler',
    'private-rpc'
];

const DOCKER_REGISTRIES = ['us-docker.pkg.dev/matterlabs-infra/matterlabs-docker', 'matterlabs'];

const UNIX_TIMESTAMP = Date.now();

async function dockerCommand(
    command: 'push' | 'build',
    image: string,
    platform: string = '',
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

    const protocolVersionTag: string = process.env.PROTOCOL_VERSION ? process.env.PROTOCOL_VERSION : '';

    // We want an alternative flow for Rust image
    if (image == 'rust') {
        await dockerCommand(command, 'server-v2', platform, customTag, dockerOrg);
        await dockerCommand(command, 'prover', platform, customTag, dockerOrg);
        return;
    }
    if (!IMAGES.includes(image)) {
        throw new Error(`Wrong image name: ${image}`);
    }

    if (image == 'keybase') {
        image = 'keybase-secret';
    }

    const tagList = customTag
        ? [customTag]
        : defaultTagList(image, COMMIT_SHORT_SHA.trim(), imageTagShaTS, protocolVersionTag);

    // Main build\push flow
    switch (command) {
        case 'build':
            await _build(image, tagList, dockerOrg, platform, buildExtraArgs);
            break;
        default:
            console.log(`Unknown command for docker ${command}.`);
            break;
    }
}

function defaultTagList(image: string, imageTagSha: string, imageTagShaTS: string, protocolVersionTag: string) {
    let tagList = [
        'server-v2',
        'external-node',
        'contract-verifier',
        'prover-fri-gateway',
        'prover-job-monitor',
        'snapshots-creator',
        'prover-autoscaler'
    ].includes(image)
        ? ['latest', 'latest2.0', `2.0-${imageTagSha}`, `${imageTagSha}`, `2.0-${imageTagShaTS}`, `${imageTagShaTS}`]
        : [`latest2.0`, 'latest'];

    if (
        protocolVersionTag &&
        [
            'proof-fri-gpu-compressor',
            'prover-fri-gateway',
            'prover-gpu-fri',
            'witness-generator',
            'witness-vector-generator',
            'circuit-prover-gpu'
        ].includes(image)
    ) {
        tagList.push(`2.0-${protocolVersionTag}-${imageTagShaTS}`, `${protocolVersionTag}-${imageTagShaTS}`);
    }

    return tagList;
}

async function _build(image: string, tagList: string[], dockerOrg: string, platform: string, extraArgs: string = '') {
    let tagsToBuild = '';

    for (const tag of tagList) {
        for (const registry of DOCKER_REGISTRIES) {
            if (platform != '') {
                let platformSuffix = platform.replace('/', '-');
                tagsToBuild = tagsToBuild + `-t ${registry}/${image}:${tag}-${platformSuffix} `;
            } else {
                tagsToBuild = tagsToBuild + `-t ${registry}/${image}:${tag} `;
            }
        }
    }

    let buildArgs = '';
    if (platform != '') {
        buildArgs += `--platform=${platform} `;
    }
    if (image === 'prover-gpu-fri' || image == 'proof-fri-gpu-compressor') {
        const cudaArch = process.env.CUDA_ARCH;
        buildArgs += `--build-arg CUDA_ARCH='${cudaArch}' `;
    }
    if (image === 'witness-generator') {
        const rustFlags = process.env.RUST_FLAGS;
        if (rustFlags) {
            buildArgs += `--build-arg RUST_FLAGS='${rustFlags}' `;
        }
    }
    buildArgs += extraArgs;

    console.log('Build args: ', buildArgs);

    const buildCommand =
        `DOCKER_BUILDKIT=1 docker buildx build ${tagsToBuild}` +
        (buildArgs ? ` ${buildArgs}` : '') +
        ` -f ./docker/${image}/Dockerfile .`;

    await utils.spawn(buildCommand);
}

export async function build(image: string, cmd: Command) {
    await dockerCommand('build', image, cmd.platform, cmd.customTag);
}

export async function customBuildForHyperchain(image: string, dockerOrg: string) {
    await dockerCommand('build', image, 'linux/amd64', dockerOrg);
}

export async function push(image: string, cmd: Command) {
    await dockerCommand('build', image, cmd.platform, cmd.customTag, '--push');
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
    .option('--platform <platform>', 'Docker platform')
    .description('build docker image')
    .action(build);
command
    .command('push <image>')
    .option('--custom-tag <value>', 'Custom tag for image')
    .option('--platform <platform>', 'Docker platform')
    .action(push);
command.command('pull').description('pull all containers').action(pull);
command.command('restart <container>').description('restart container in docker-compose.yml').action(restart);
