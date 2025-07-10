import * as fs from "fs";
import * as path from "path";

// All ZKsync test suites are expected to be named `*.test.ts`.
const TEST_SUITE_MARKER = "test.ts";

// Files that are excluded from the integration test suited (e.g. unit tests for the framework itself).
const EXCLUDED_FILES = ["self-unit.test.ts"];

/**
 * Gets all the files that contain ZKsync integration test suites.
 * Used to provide each test suite a funded wallet.
 *
 * @returns list of filenames that correspond to ZKsync integration test suites.
 */
export function lookupPrerequisites(): string[] {
  const files = loadFilesRecursively(`${__dirname}/../tests/`);

  return files.filter(
    (file) =>
      !EXCLUDED_FILES.includes(file) && file.endsWith(TEST_SUITE_MARKER),
  );
}

/**
 * Recursively collects file paths from the `base` directory (e.g. `some/directory/file.ts`)
 *
 * @param base Base folder to recursively traverse.
 * @param dirPath Collected path relative to the base folder.
 * @param arrayOfFiles Array of files collected so far.
 * @returns Array of file paths.
 */
function loadFilesRecursively(
  base: string,
  dirPath: string = "",
  arrayOfFiles: string[] = [],
): string[] {
  const files = fs.readdirSync(base + dirPath);

  files.forEach((file) => {
    if (fs.statSync(base + dirPath + "/" + file).isDirectory()) {
      arrayOfFiles = loadFilesRecursively(
        base,
        dirPath + "/" + file,
        arrayOfFiles,
      );
    } else {
      const relativePath = path.join(dirPath, "/", file).substring(1); // strip the `/` at the beginning.
      arrayOfFiles.push(relativePath);
    }
  });

  return arrayOfFiles;
}
