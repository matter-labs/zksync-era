import { Command } from 'commander';
import * as utils from './utils';
import fs from 'fs';
import enquirer from 'enquirer';
import { BasePromptOptions } from './hyperchain_wizard';
import fetch from 'node-fetch';
import chalk from 'chalk';
import * as env from './env';

export enum ProverType {
    CPU = 'cpu',
    GPU = 'gpu'
}

export async function setupProver(proverType: ProverType) {
    // avoid doing work if receives the wrong param from the CLI
    if (proverType == ProverType.GPU || proverType == ProverType.CPU) {
        env.modify('PROVER_TYPE', proverType, process.env.ENV_FILE!);
        env.modify('ETH_SENDER_SENDER_PROOF_SENDING_MODE', 'OnlyRealProofs', process.env.ENV_FILE!);
        env.modify('ETH_SENDER_SENDER_PROOF_LOADING_MODE', 'FriProofFromGcs', process.env.ENV_FILE!);
        env.modify('FRI_PROVER_GATEWAY_API_POLL_DURATION_SECS', '120', process.env.ENV_FILE!);
        await setupArtifactsMode();
        if (!process.env.CI) {
            await setupProverKeys(proverType);
        } else {
            env.modify(
                'FRI_PROVER_SETUP_DATA_PATH',
                `${process.env.ZKSYNC_HOME}/etc/hyperchains/prover-keys/${process.env.ZKSYNC_ENV}/${
                    proverType === ProverType.GPU ? 'gpu' : 'cpu'
                }/`,
                process.env.ENV_FILE!
            );
        }
        env.mergeInitToEnv();
    } else {
        console.error(`Unknown prover type: ${proverType}`);
        process.exit(1);
    }
}

async function downloadCSR(proverType: ProverType) {
    const currentEnv = env.get();
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/etc/hyperchains/prover-keys/${currentEnv}/${proverType}/`, {
        recursive: true
    });
    process.chdir(`${process.env.ZKSYNC_HOME}/etc/hyperchains/prover-keys/${currentEnv}/${proverType}/`);
    console.log(chalk.yellow('Downloading ceremony (CSR) file'));
    await utils.spawn('wget -c https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2^24.key');
    await utils.sleep(1);
    process.chdir(process.env.ZKSYNC_HOME as string);
    env.modify(
        'CRS_FILE',
        `${process.env.ZKSYNC_HOME}/etc/hyperchains/prover-keys/${currentEnv}/${proverType}/`,
        process.env.ENV_FILE!
    );
}

async function setupProverKeys(proverType: ProverType) {
    const DOWNLOAD = 'Download default keys';
    const GENERATE = 'Generate locally';
    const questions: BasePromptOptions[] = [
        {
            message:
                'Do you want to download default Boojum prover setup keys, or generate them locally (takes some time - only needed if you changed anything on the prover code)?',
            name: 'proverKeys',
            type: 'select',
            choices: [DOWNLOAD, GENERATE]
        }
    ];

    const results: any = await enquirer.prompt(questions);

    await downloadCSR(proverType);
    if (results.proverKeys == DOWNLOAD) {
        await downloadDefaultSetupKeys(proverType);
    } else {
        await generateAllSetupData(proverType);
    }

    env.modify(
        'FRI_PROVER_SETUP_DATA_PATH',
        `${process.env.ZKSYNC_HOME}/etc/hyperchains/prover-keys/${process.env.ZKSYNC_ENV}/${
            proverType === ProverType.GPU ? 'gpu' : 'cpu'
        }/`,
        process.env.ENV_FILE!
    );
}

async function setupArtifactsMode() {
    if (process.env.CI) {
        const currentEnv = env.get();
        const path = `${process.env.ZKSYNC_HOME}/etc/hyperchains/artifacts/${currentEnv}/`;
        env.modify('OBJECT_STORE_MODE', 'FileBacked', process.env.ENV_FILE!);
        env.modify('PUBLIC_OBJECT_STORE_MODE', 'FileBacked', process.env.ENV_FILE!);
        env.modify('PROVER_OBJECT_STORE_MODE', 'FileBacked', process.env.ENV_FILE!);
        env.modify('OBJECT_STORE_FILE_BACKED_BASE_PATH', path, process.env.ENV_FILE!);
        env.modify('PUBLIC_OBJECT_STORE_FILE_BACKED_BASE_PATH', path, process.env.ENV_FILE!);
        env.modify('PROVER_OBJECT_STORE_FILE_BACKED_BASE_PATH', path, process.env.ENV_FILE!);
        return;
    }

    const LOCAL = 'Local folder';
    const GCP = 'GCP';
    const questions: BasePromptOptions[] = [
        {
            message: 'Will you use a local folder for storing prover artifacts, or Google Cloud Platform (GCP)?',
            name: 'mode',
            type: 'select',
            choices: [LOCAL, GCP]
        }
    ];

    const results: any = await enquirer.prompt(questions);

    if (results.mode == LOCAL) {
        const currentEnv = env.get();

        const folderQuestion: BasePromptOptions[] = [
            {
                message: 'Please select the path to store the proving process artifacts.',
                name: 'path',
                type: 'input',
                required: true,
                initial: `${process.env.ZKSYNC_HOME}/etc/hyperchains/artifacts/${currentEnv}/`
            }
        ];

        const folder: any = await enquirer.prompt(folderQuestion);

        env.modify('OBJECT_STORE_MODE', 'FileBacked', process.env.ENV_FILE!);
        env.modify('PUBLIC_OBJECT_STORE_MODE', 'FileBacked', process.env.ENV_FILE!);
        env.modify('PROVER_OBJECT_STORE_MODE', 'FileBacked', process.env.ENV_FILE!);
        env.modify('OBJECT_STORE_FILE_BACKED_BASE_PATH', folder.path, process.env.ENV_FILE!);
        env.modify('PUBLIC_OBJECT_STORE_FILE_BACKED_BASE_PATH', folder.path, process.env.ENV_FILE!);
        env.modify('PROVER_OBJECT_STORE_FILE_BACKED_BASE_PATH', folder.path, process.env.ENV_FILE!);
    } else {
        const gcpQuestions: BasePromptOptions[] = [
            {
                message: 'Please provide the path for a GCP credential file.',
                name: 'gcpPath',
                type: 'input',
                required: true
            },
            {
                message: 'Please provide the bucket name on GCP where artifacts should be stored.',
                name: 'bucket',
                type: 'input',
                required: true
            }
        ];

        const gcp: any = await enquirer.prompt(gcpQuestions);

        env.modify('OBJECT_STORE_MODE', 'GCSWithCredentialFile', process.env.ENV_FILE!);
        env.modify('PUBLIC_OBJECT_STORE_MODE', 'GCSWithCredentialFile', process.env.ENV_FILE!);
        env.modify('PROVER_OBJECT_STORE_MODE', 'GCSWithCredentialFile', process.env.ENV_FILE!);
        env.modify('OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH', gcp.gcpPath, process.env.ENV_FILE!);
        env.modify('PUBLIC_OBJECT_STORE_GCS_CREDENTIAL_FILE_PATH', gcp.gcpPath, process.env.ENV_FILE!);
        env.modify('PUBLIC_OBJECT_STORE_BUCKET_BASE_URL', gcp.bucket, process.env.ENV_FILE!);
        env.modify('PROVER_OBJECT_STORE_BUCKET_BASE_URL', gcp.bucket, process.env.ENV_FILE!);
    }
}

async function generateSetupDataForBaseLayer(proverType: ProverType) {
    await generateSetupData(true, proverType);
}

async function generateSetupDataForRecursiveLayers(proverType: ProverType) {
    await generateSetupData(false, proverType);
}

async function generateSetupData(isBaseLayer: boolean, proverType: ProverType) {
    const currentEnv = env.get();
    fs.mkdirSync(`${process.env.ZKSYNC_HOME}/etc/hyperchains/prover-keys/${currentEnv}/${proverType}/`, {
        recursive: true
    });
    process.chdir(`${process.env.ZKSYNC_HOME}/prover`);
    await utils.spawn(
        `for i in {1..${isBaseLayer ? '13' : '15'}}; do zk f cargo run ${
            proverType == ProverType.GPU ? '--features "gpu"' : ''
        } --release --bin zksync_setup_data_generator_fri -- --numeric-circuit $i ${
            isBaseLayer ? '--is_base_layer' : ''
        }; done`
    );
    process.chdir(process.env.ZKSYNC_HOME as string);
}

async function generateAllSetupData(proverType: ProverType) {
    await generateSetupDataForBaseLayer(proverType);
    await generateSetupDataForRecursiveLayers(proverType);
}

async function downloadDefaultSetupKeys(proverType: ProverType, region: 'us' | 'asia' | 'europe' = 'us') {
    const proverKeysUrls = require(`${process.env.ZKSYNC_HOME}/prover/setup-data-${proverType}-keys.json`);
    const currentEnv = env.get();
    await downloadFilesFromGCP(
        proverKeysUrls[region],
        `${process.env.ZKSYNC_HOME}/etc/hyperchains/prover-keys/${currentEnv}/${proverType}/`
    );

    await utils.spawn(
        `cp -r ${process.env.ZKSYNC_HOME}/prover/vk_setup_data_generator_server_fri/data/* ${process.env.ZKSYNC_HOME}/etc/hyperchains/prover-keys/${currentEnv}/${proverType}/`
    );
}

async function listFilesFromGCP(gcpUri: string): Promise<string[]> {
    const matches = gcpUri.match(/gs:\/\/([^\/]*)\/([^\/]*)\/?/);
    if (matches != null) {
        const url = `https://storage.googleapis.com/storage/v1/b/${matches[1]}/o?prefix=${matches[2]}%2F`;
        const response = await fetch(url);
        if (response.ok) {
            const json = await response.json();
            return json.items.map((item: any) => `https://storage.googleapis.com/${matches[1]}/${item.name}`);
        }
    }
    return [];
}

async function downloadFilesFromGCP(gcpUri: string, destination: string): Promise<void> {
    const files = await listFilesFromGCP(gcpUri);

    fs.mkdirSync(destination, { recursive: true });
    process.chdir(destination);

    const length = files.length;
    for (const index in files) {
        console.log(chalk.yellow(`Downloading file ${Number(index) + 1} of ${length}`));
        const file = files[index];
        await utils.spawn(`wget -c ${file}`);
        await utils.sleep(1);
        console.log(``);
    }
    process.chdir(process.env.ZKSYNC_HOME as string);
}

export const proverCommand = new Command('prover').description('Prover setup related commands');

proverCommand
    .command('setup')
    .arguments('[type]')
    .action((type: ProverType) => setupProver(type));
