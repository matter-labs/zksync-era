import { Command } from 'commander';
import * as utils from './utils';
import * as contract from './contract';

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
    'proof-fri-compressor'
];
const UNIX_TIMESTAMP = Date.now();

async function dockerCommand(
    command: 'push' | 'build',
    image: string,
    customTag?: string,
    publishPublic: boolean = false
) {
    // Generating all tags for containers. We need 2 tags here: SHA and SHA+TS
    const { stdout: COMMIT_SHORT_SHA }: { stdout: string } = await utils.exec('git rev-parse --short HEAD');
    const imageTagShaTS: string = process.env.IMAGE_TAG_SUFFIX
        ? process.env.IMAGE_TAG_SUFFIX
        : `${COMMIT_SHORT_SHA.trim()}-${UNIX_TIMESTAMP}`;

    // we want alternative flow for rust image
    if (image == 'rust') {
        await dockerCommand(command, 'server-v2', customTag, publishPublic);
        await dockerCommand(command, 'prover', customTag, publishPublic);
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
    // COMMIT_SHORT_SHA returns with newline, so we need to trim it
    switch (command) {
        case 'build':
            await _build(image, tagList);
            break;
        case 'push':
            await _push(image, tagList, publishPublic);
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
        'proof-fri-compressor'
    ].includes(image)
        ? ['latest', 'latest2.0', `2.0-${imageTagSha}`, `${imageTagSha}`, `2.0-${imageTagShaTS}`, `${imageTagShaTS}`]
        : [`latest2.0`, 'latest'];

    return tagList;
}

async function _build(image: string, tagList: string[]) {
    if (image === 'server-v2' || image === 'external-node' || image === 'prover') {
        await contract.build();
    }

    const tagsToBuild = tagList.map((tag) => `-t matterlabs/${image}:${tag}`).join(' ');
    // generate list of tags for image - we want 3 tags (latest, SHA, SHA+TimeStamp) for listed components and only "latest" for everything else

    // Conditionally add build argument if image is prover-v2
    let buildArgs = '';
    if (image === 'prover-v2') {
        const eraBellmanCudaRelease = process.env.ERA_BELLMAN_CUDA_RELEASE;
        buildArgs = `--build-arg ERA_BELLMAN_CUDA_RELEASE=${eraBellmanCudaRelease}`;
    }

    // HACK
    // For prover-v2 which is not a prover, but should be built from the prover dockerfile. So here we go.
    const imagePath = image == 'prover-v2' ? 'prover' : image;

    const buildCommand =
        `DOCKER_BUILDKIT=1 docker build ${tagsToBuild}` +
        (buildArgs ? ` ${buildArgs}` : '') +
        ` -f ./docker/${imagePath}/Dockerfile .`;

    await utils.spawn(buildCommand);
}

async function _push(image: string, tagList: string[], publishPublic: boolean = false) {
    // For development purposes, we want to use `2.0` tags for 2.0 images, just to not interfere with 1.x

    for (const tag of tagList) {
        await utils.spawn(`docker push matterlabs/${image}:${tag}`);
        await utils.spawn(
            `docker tag matterlabs/${image}:${tag} us-docker.pkg.dev/matterlabs-infra/matterlabs-docker/${image}:${tag}`
        );
        await utils.spawn(`docker push us-docker.pkg.dev/matterlabs-infra/matterlabs-docker/${image}:${tag}`);

        if (image == 'circuit-synthesizer') {
            await utils.spawn(
                `docker tag us-docker.pkg.dev/matterlabs-infra/matterlabs-docker/${image}:${tag} asia-docker.pkg.dev/matterlabs-infra/matterlabs-docker/${image}:${tag}`
            );
            await utils.spawn(`docker push asia-docker.pkg.dev/matterlabs-infra/matterlabs-docker/${image}:${tag}`);
        }
        if (image == 'external-node' && publishPublic) {
            await utils.spawn(`docker push matterlabs/${image}-public:${tag}`);
        }
    }
}

export async function build(image: string, cmd: Command) {
    await dockerCommand('build', image, cmd.customTag);
}

export async function push(image: string, cmd: Command) {
    await dockerCommand('build', image, cmd.customTag, cmd.public);
    await dockerCommand('push', image, cmd.customTag, cmd.public);
}

export async function restart(container: string) {
    await utils.spawn(`docker-compose restart ${container}`);
}

export async function pull() {
    await utils.spawn('docker-compose pull');
}

export const command = new Command('docker').description('docker management');

command
    .command('build <image>')
    .option('--custom-tag <value>', 'Custom tag for image')
    .description('build docker image')
    .action(build);
command
    .command('push <image>')
    .option('--custom-tag <value>', 'Custom tag for image')
    .option('--public', 'Publish image to the public repo')
    .description('build and push docker image')
    .action(push);
command.command('pull').description('pull all containers').action(pull);
command.command('restart <container>').description('restart container in docker-compose.yml').action(restart);
