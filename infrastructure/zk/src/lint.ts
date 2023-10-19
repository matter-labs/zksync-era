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
    if (extension == 'rust') {
        await clippy();
        return;
    }

    if (extension == 'prover') {
        await proverClippy();
        return;
    }

    if (!EXTENSIONS.includes(extension)) {
        throw new Error('Unsupported extension');
    }

    const files = await utils.getUnignoredFiles(extension);
    const command = LINT_COMMANDS[extension];
    const fixOption = check ? '' : '--fix';

    await utils.spawn(`yarn --silent ${command} ${fixOption} --config ${CONFIG_PATH}/${extension}.js ${files}`);
}

async function clippy() {
    process.chdir(process.env.ZKSYNC_HOME!);
    await utils.spawn('cargo clippy --tests -- -D warnings');
}

async function proverClippy() {
    process.chdir(process.env.ZKSYNC_HOME! + '/prover');
    await utils.spawn('cargo clippy --tests -- -D warnings -A incomplete_features');
}

export const command = new Command('lint')
    .description('lint code')
    .option('--check')
    .arguments('[extension] rust|md|sol|js|ts|prover')
    .action(async (extension: string | null, cmd: Command) => {
        if (extension) {
            await lint(extension, cmd.check);
        } else {
            for (const ext of EXTENSIONS) {
                await lint(ext, cmd.check);
            }

            await clippy();
        }
    });
