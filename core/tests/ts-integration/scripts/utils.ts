import * as fs from 'fs';
import * as fsPr from 'fs/promises';
import path from 'path';
import { exec } from 'child_process';

const CONTRACTS_DIR = 'contracts';
const OUTPUT_DIR = 'artifacts-zk';
const TIMESTAMP_FILE = 'last_compilation.timestamp'; // File to store the last compilation time
const TIMESTAMP_FILE_YUL = 'last_compilation_yul.timestamp'; // File to store the last compilation time

// Get the latest file modification time in the watched folder
function getLatestModificationTime(folder: string): Date | null {
    const files = fs.readdirSync(folder);
    let latestTime: Date | null = null;

    files.forEach((file) => {
        const filePath = path.join(folder, file);
        const stats = fs.statSync(filePath);
        if (stats.isDirectory()) {
            const dirLatestTime = getLatestModificationTime(filePath);
            if (dirLatestTime && (!latestTime || dirLatestTime > latestTime)) {
                latestTime = dirLatestTime;
            }
        } else if (stats.isFile()) {
            if (!latestTime || stats.mtime > latestTime) {
                latestTime = stats.mtime;
            }
        }
    });

    return latestTime;
}

// Read the last compilation timestamp from the file
function getLastCompilationTime(timestampFile: string): Date | null {
    try {
        if (fs.existsSync(timestampFile)) {
            const timestamp = fs.readFileSync(timestampFile, 'utf-8');
            return new Date(parseInt(timestamp, 10));
        }
    } catch (error) {
        const err = error as Error; // Cast `error` to `Error`
        console.error(`Error reading timestamp: ${err.message}`);
    }
    return null;
}

// Write the current time to the timestamp file
export function setCompilationTime(timestampFile: string) {
    fs.writeFileSync(timestampFile, Date.now().toString());
}

// Determine if recompilation is needed
export function needsRecompilation(folder: string, timestampFile: string): boolean {
    const lastCompilationTime = getLastCompilationTime(timestampFile);
    const latestModificationTime = getLatestModificationTime(folder);

    if (!latestModificationTime || !lastCompilationTime) {
        return true; // If there's no history, always recompile
    }

    return latestModificationTime > lastCompilationTime;
}

export function deleteDir(path: string): Promise<void> {
    return new Promise((resolve, reject) => {
        exec(`rm -rf ${path}`, (error) => {
            if (error) {
                reject(error); // If an error occurs, reject the promise
            } else {
                resolve(); // If successful, resolve the promise
            }
        });
    });
}

export async function isFolderEmpty(folderPath: string): Promise<boolean> {
    try {
        const files = await fsPr.readdir(folderPath); // Get a list of files in the folder
        return files.length === 0; // If there are no files, the folder is empty
    } catch (error) {
        console.error('No target folder with artifacts.');
        return true; // Return true if an error, as folder doesn't exist.
    }
}

export { CONTRACTS_DIR, OUTPUT_DIR, TIMESTAMP_FILE, TIMESTAMP_FILE_YUL };
