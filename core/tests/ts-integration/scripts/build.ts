import path from 'path';
import { exec } from 'child_process';
import { needsRecompilation, setCompilationTime, isFolderEmpty } from './utils';

const CONTRACTS_DIR = 'contracts';
const OUTPUT_DIR = 'artifacts-zk';
const TIMESTAMP_FILE = 'last_compilation.timestamp'; // File to store the last compilation time

async function main() {
    const timestampFilePath = path.join(process.cwd(), TIMESTAMP_FILE);
    const folderToCheck = path.join(process.cwd(), CONTRACTS_DIR);

    if ((await isFolderEmpty(OUTPUT_DIR)) || needsRecompilation(folderToCheck, timestampFilePath)) {
        console.log('Compilation needed.');
        exec(`hardhat compile`, (error) => {
            if (error) {
                throw error;
            } else {
                console.log('Compilation successful.');
            }
        });
        setCompilationTime(timestampFilePath);
    } else {
        console.log('Compilation not needed.');
        return;
    }
}

main();
