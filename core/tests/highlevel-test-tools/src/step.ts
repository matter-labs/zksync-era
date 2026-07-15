// test/helpers/step.ts
import chalk from 'chalk';

let depth = 0; // supports nested steps

export default async function step<T>(title: string, body: () => Promise<T> | T): Promise<T> {
    const indent = '  '.repeat(depth);
    console.log(indent + chalk.blue('→ ' + title)); // before

    depth++;
    try {
        const result = await body();
        depth--;
        console.log(indent + chalk.green('✓ ' + title)); // success
        return result;
    } catch (err) {
        depth--;
        console.log(indent + chalk.red('✗ ' + title)); // failure
        throw err; // let Vitest mark the test failed
    }
}
