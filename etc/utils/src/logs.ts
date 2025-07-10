import path from "path";
import fs from "node:fs/promises";

const pathToHome = path.join(__dirname, "../../..");

export async function logsTestPath(
  chain: string | undefined,
  relativePath: string,
  name: string,
): Promise<string> {
  chain = chain ?? "default";
  let dir = path.join(pathToHome, relativePath, chain);
  await fs.mkdir(dir, { recursive: true });
  return path.join(dir, name);
}
