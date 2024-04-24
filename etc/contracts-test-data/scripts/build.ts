import path from 'path';
import {
    needsRecompilation,
    setCompilationTime,
    isFolderEmpty,
    CONTRACTS_DIR,
    OUTPUT_DIR,
    TIMESTAMP_FILE
} from './utils';
import { exec } from 'child_process';

async function main() {
    const timestampFilePath = path.join(process.cwd(), TIMESTAMP_FILE);
    const folderToCheck = path.join(process.cwd(), CONTRACTS_DIR);

    if ((await isFolderEmpty(OUTPUT_DIR)) || needsRecompilation(folderToCheck, timestampFilePath)) {
        console.log('Compilation needed.');
        // Perform recompilation (e.g., run hardhat, truffle, etc.)
        exec(`hardhat compile`, (error) => {
            if (error) {
                throw error; // If an error occurs, reject the promise
            } else {
                console.log('Compilation successful.');
            }
        });
        setCompilationTime(timestampFilePath); // Update the timestamp after recompilation
    } else {
        console.log('Compilation not needed.');
        return;
    }
}

main();
