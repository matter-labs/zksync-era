import { Command } from 'commander';
import * as utils from './utils';

// Note that `rust` is not noted here, as clippy isn't run via `yarn`.
// `rust` option is still supported though.
const LINT_COMMANDS: Record<string, string> = {
    md: 'markdownlint',
    sol: 'solhint',
    js: 'eslint',
    ts: 'eslint --ext ts'
};
const EXTENSIONS = Object.keys(LINT_COMMANDS);
const CONFIG_PATH = 'etc/lint-config';

export async function lint(extension: string, check: boolean = false) {
    if (!EXTENSIONS.includes(extension)) {
        throw new Error('Unsupported extension');
    }

    const files = await utils.getUnignoredFiles(extension);
    const command = LINT_COMMANDS[extension];
    const fixOption = check ? '' : '--fix';

    await utils.spawn(`yarn --silent ${command} ${fixOption} --config ${CONFIG_PATH}/${extension}.js ${files}`);
}

async function lintL1Contracts(check: boolean = false) {
    await utils.spawn(`yarn --silent --cwd contracts/ethereum lint:${check ? 'check' : 'fix'}`);
}

async function lintL2Contracts(check: boolean = false) {
    await utils.spawn(`yarn --silent --cwd contracts/zksync lint:${check ? 'check' : 'fix'}`);
}

async function lintSystemContracts(check: boolean = false) {
    await utils.spawn(`yarn --silent --cwd etc/system-contracts lint:${check ? 'check' : 'fix'}`);
}

async function clippy() {
    process.chdir(process.env.ZKSYNC_HOME!);
    const { stdout: rustVersion } = await utils.exec('rustc --version');
    console.log(`linting using rustc: ${rustVersion.trim()}`);
    const { stdout: version } = await utils.exec('cargo clippy --version');
    console.log(`linting using clippy: ${version.trim()}`);
    await utils.spawn('cargo clippy --tests -- -D warnings');
}

async function proverClippy() {
    process.chdir(process.env.ZKSYNC_HOME! + '/prover');
    await utils.spawn('cargo clippy --tests -- -D warnings -A incomplete_features');
}

const ARGS = [...EXTENSIONS, 'rust', 'prover', 'l1-contracts', 'l2-contracts', 'system-contracts'];

export const command = new Command('lint')
    .description('lint code')
    .option('--check')
    .arguments(`[extension] ${ARGS.join('|')}`)
    .action(async (extension: string | null, cmd: Command) => {
        if (extension) {
            switch (extension) {
                case 'rust':
                    await clippy();
                    break;
                case 'prover':
                    await proverClippy();
                    break;
                case 'l1-contracts':
                    await lintL1Contracts(cmd.check);
                    break;
                case 'l2-contracts':
                    await lintL2Contracts(cmd.check);
                    break;
                case 'system-contracts':
                    await lintSystemContracts(cmd.check);
                    break;
                default:
                    await lint(extension, cmd.check);
            }
        } else {
            const promises = EXTENSIONS.map((ext) => lint(ext, cmd.check));
            promises.push(lintL1Contracts(cmd.check));
            promises.push(lintL2Contracts(cmd.check));
            promises.push(lintSystemContracts(cmd.check));
            promises.push(clippy());
            await Promise.all(promises);
        }
    });
