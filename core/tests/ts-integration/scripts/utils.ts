import * as fs from 'fs';
import path from 'path';
import { exec } from 'child_process';
import { glob } from 'glob';

const CONTRACTS_DIR = 'contracts';
const OUTPUT_DIR = 'artifacts-zk';
const TIMESTAMP_FILE = 'last_compilation.timestamp'; // File to store the last compilation time
const TIMESTAMP_FILE_YUL = 'last_compilation_yul.timestamp'; // File to store the last compilation time

// Function to check if a file exists
function fileExists(filePath: string): boolean {
    try {
        return fs.existsSync(filePath);
    } catch {
        return false;
    }
}

/**
 * Resolves the import path, handling relative imports and node module imports.
 * @param currentDir The current directory from which the import originates.
 * @param importPath The import path to resolve.
 * @returns The resolved absolute path to the imported file.
 */
function resolveImportPath(currentDir: string, importPath: string): string {
    if (importPath.startsWith('.') || importPath.startsWith('/')) {
        // Relative or absolute path
        return path.resolve(currentDir, importPath);
    } else {
        // Likely a node module
        return require.resolve(importPath, { paths: [process.cwd()] });
    }
}

/**
 * Get the list of imported Solidity files recursively, considering both relative and node module imports.
 * @param filePath The path of the Solidity file to analyze.
 * @param seenFiles A set to track already seen files to avoid loops.
 * @returns A set of unique file paths that are imported.
 */
function getImportedFiles(filePath: string, seenFiles: Set<string> = new Set()): Set<string> {
    const imports = new Set<string>();

    if (!fileExists(filePath)) {
        console.error(`File not found: ${filePath}`);
        return imports; // Return empty set if the file doesn't exist
    }

    const content = fs.readFileSync(filePath, 'utf-8');
    const importRegex = /import ["'](.+?)["']/g;

    let match: RegExpExecArray | null;
    const currentDir = path.dirname(filePath);
    while ((match = importRegex.exec(content)) !== null) {
        const importedFile = resolveImportPath(currentDir, match[1]);

        if (!seenFiles.has(importedFile)) {
            seenFiles.add(importedFile);
            imports.add(importedFile);

            // Recursively find imports within this imported file
            if (fileExists(importedFile)) {
                getImportedFiles(importedFile, seenFiles).forEach((file) => imports.add(file));
            } else {
                console.error(`Imported file not found: ${importedFile}`);
            }
        }
    }

    return imports;
}

/**
 * Get the latest modification time among all Solidity files and their imports.
 * @param folder The root folder to start the search.
 * @returns The latest modification time among all Solidity files.
 */
function getLatestModificationTime(folder: string): Date {
    const files = glob.sync(`${folder}/**/*.sol`); // Find all Solidity files
    let latestTime: Date | null = null;

    files.forEach((filePath) => {
        const allFiles = getImportedFiles(filePath);

        allFiles.forEach((file) => {
            if (!fileExists(file)) {
                console.error(`File not found: ${file}`);
                return; // Continue to the next file if this one doesn't exist
            }

            const stats = fs.statSync(file);
            if (!latestTime || stats.mtime > latestTime) {
                latestTime = stats.mtime;
            }
        });
    });

    if (latestTime) {
        return latestTime;
    } else {
        throw new Error('No Solidity files found or no modification times detected.');
    }
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

export function isFolderEmpty(folderPath: string): boolean {
    try {
        const files = fs.readdirSync(folderPath); // Read the folder synchronously
        return files.length === 0; // If there are no files, the folder is empty
    } catch (error) {
        const err = error as Error; // Cast `error` to `Error`
        console.error('Error while checking if folder is empty:', err.message);
        return true; // If an error occurs (like the folder doesn't exist), assume it's empty
    }
}

export { CONTRACTS_DIR, OUTPUT_DIR, TIMESTAMP_FILE, TIMESTAMP_FILE_YUL };
