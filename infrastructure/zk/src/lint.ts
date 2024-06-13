import { Command } from 'commander';
import * as utils from 'utils';

// Note that `rust` is not noted here, as clippy isn't run via `yarn`.
// `rust` option is still supported though.
const LINT_COMMANDS = {
    md: 'markdownlint',
    sol: 'solhint',
    js: 'eslint',
    ts: 'eslint --ext ts'
};
const EXTENSIONS = Object.keys(LINT_COMMANDS) as (keyof typeof LINT_COMMANDS)[];
const CONFIG_PATH = 'etc/lint-config';

export async function lint(extension: keyof typeof LINT_COMMANDS, check: boolean = false) {
    if (!EXTENSIONS.includes(extension)) {
        throw new Error('Unsupported extension');
    }

    const files = await utils.getUnignoredFiles(extension);
    const command = LINT_COMMANDS[extension];
    const fixOption = check ? '' : '--fix';

    await utils.spawn(`yarn --silent ${command} ${fixOption} --config ${CONFIG_PATH}/${extension}.js ${files}`);
}

async function lintContracts(check: boolean = false) {
    await utils.spawn(`yarn --silent --cwd contracts lint:${check ? 'check' : 'fix'}`);
}

async function clippy() {
    process.chdir(process.env.ZKSYNC_HOME!);
    await utils.spawn('cargo clippy --tests --locked -- -D warnings -D unstable_features');
}

async function proverClippy() {
    process.chdir(`${process.env.ZKSYNC_HOME}/prover`);
    await utils.spawn('cargo clippy --tests --locked -- -D warnings -A incomplete_features');
}

async function toolboxClippy() {
    process.chdir(`${process.env.ZKSYNC_HOME}/zk_toolbox`);
    await utils.spawn('cargo clippy --tests --locked -- -D warnings');
}

const ARGS = [...EXTENSIONS, 'rust', 'prover', 'contracts', 'toolbox'] as const;

export const command = new Command('lint')
    .description('lint code')
    .option('--check')
    .arguments(`[extension] ${ARGS.join('|')}`)
    .action(async (extension: (typeof ARGS)[number] | null, cmd: Command) => {
        if (extension) {
            switch (extension) {
                case 'rust':
                    await clippy();
                    break;
                case 'prover':
                    await proverClippy();
                    break;
                case 'contracts':
                    await lintContracts(cmd.check);
                    break;
                case 'toolbox':
                    await toolboxClippy();
                    break;
                default:
                    await lint(extension, cmd.check);
            }
        } else {
            const promises = EXTENSIONS.map((ext) => lint(ext, cmd.check));
            promises.push(lintContracts(cmd.check));
            promises.push(clippy());
            promises.push(proverClippy());
            promises.push(toolboxClippy());
            await Promise.all(promises);
        }
    });
