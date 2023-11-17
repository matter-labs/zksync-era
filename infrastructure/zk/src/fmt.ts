import { Command } from 'commander';
import * as utils from './utils';

const EXTENSIONS = ['ts', 'md', 'sol', 'js'];
const CONFIG_PATH = 'etc/prettier-config';

function prettierFlags(phaseName: string) {
    phaseName = phaseName.replace('/', '-').replace('.', '');
    return ` --cache --cache-location node_modules/.cache/prettier/.prettier-cache-${phaseName}`;
}
export async function prettier(extension: string, check: boolean = false) {
    if (!EXTENSIONS.includes(extension)) {
        throw new Error('Unsupported extension');
    }

    const command = check ? 'check' : 'write';
    const files = await utils.getUnignoredFiles(extension);

    if (files.length === 0) {
        console.log(`No files of extension ${extension} to format`);
        return;
    }

    await utils.spawn(
        `yarn --silent prettier --config ${CONFIG_PATH}/${extension}.js --${command} ${files} ${prettierFlags(
            extension
        )}  ${check ? '' : '> /dev/null'}`
    );
}

async function prettierContracts(check: boolean, directory: string) {
    await utils.spawn(
        `yarn --silent --cwd ${directory} prettier:${check ? 'check' : 'fix'} ${prettierFlags(directory)} ${
            check ? '' : '> /dev/null'
        }`
    );
}

async function prettierL1Contracts(check: boolean = false) {
    await prettierContracts(check, 'contracts/ethereum');
}

async function prettierL2Contracts(check: boolean = false) {
    await prettierContracts(check, 'contracts/zksync');
}

async function prettierSystemContracts(check: boolean = false) {
    await prettierContracts(check, 'etc/system-contracts');
}

export async function rustfmt(check: boolean = false) {
    process.chdir(process.env.ZKSYNC_HOME as string);
    const command = check ? 'cargo fmt -- --check' : 'cargo fmt';
    await utils.spawn(command);
    process.chdir('./prover');
    await utils.spawn(command);
}

const ARGS = [...EXTENSIONS, 'rust', 'l1-contracts', 'l2-contracts', 'system-contracts'];

export const command = new Command('fmt')
    .description('format code with prettier & rustfmt')
    .option('--check')
    .arguments(`[extension] ${ARGS.join('|')}`)
    .action(async (extension: string | null, cmd: Command) => {
        if (extension) {
            switch (extension) {
                case 'rust':
                    await rustfmt(cmd.check);
                    break;
                case 'l1-contracts':
                    await prettierL1Contracts(cmd.check);
                    break;
                case 'l2-contracts':
                    await prettierL2Contracts(cmd.check);
                    break;
                case 'system-contracts':
                    await prettierSystemContracts(cmd.check);
                    break;
                default:
                    await prettier(extension, cmd.check);
                    break;
            }
        } else {
            // Run all the checks concurrently.
            const promises = EXTENSIONS.map((ext) => prettier(ext, cmd.check));
            promises.push(rustfmt(cmd.check));
            promises.push(prettierL1Contracts(cmd.check));
            promises.push(prettierL2Contracts(cmd.check));
            promises.push(prettierSystemContracts(cmd.check));
            await Promise.all(promises);
        }
    });

command
    .command('prettier')
    .option('--check')
    .arguments('[extension]')
    .action(async (extension: string | null, cmd: Command) => {
        if (extension) {
            await prettier(extension, cmd.check);
        } else {
            for (const ext of EXTENSIONS) {
                await prettier(ext, cmd.check);
            }
        }
    });

command
    .command('rustfmt')
    .option('--check')
    .action(async (cmd: Command) => {
        await rustfmt(cmd.check);
    });
