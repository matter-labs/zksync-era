import * as path from "path";
import * as fs from "fs";
import * as yaml from "yaml";
import fsSync from "fs";

export type FileConfig =
  | { loadFromFile: false; chain?: undefined }
  | { loadFromFile: true; chain: string };

export function shouldLoadConfigFromFile(): FileConfig {
  const chain = process.env.CHAIN_NAME;
  return chain ? { loadFromFile: true, chain } : { loadFromFile: false };
}

export const configNames = [
  "contracts.yaml",
  "general.yaml",
  "genesis.yaml",
  "secrets.yaml",
  "wallets.yaml",
  "external_node.yaml",
  "gateway_chain.yaml",
] as const;

export type ConfigName = (typeof configNames)[number];

export function loadEcosystem(pathToHome: string) {
  const configPath = path.join(pathToHome, "/ZkStack.yaml");
  if (!fs.existsSync(configPath)) {
    return [];
  }
  return yaml.parse(
    fs.readFileSync(configPath, {
      encoding: "utf-8",
    }),
  );
}

export function loadChainConfig(pathToHome: string, chain: string) {
  const configPath = path.join(pathToHome, "chains", chain, "/ZkStack.yaml");

  if (!fs.existsSync(configPath)) {
    return [];
  }
  return yaml.parse(
    fs.readFileSync(configPath, {
      encoding: "utf-8",
    }),
  );
}

export function loadConfig({
  pathToHome,
  chain,
  configsFolder,
  configsFolderSuffix,
  config,
}: {
  pathToHome: string;
  chain: string;
  configsFolder?: string;
  configsFolderSuffix?: string;
  config: ConfigName;
}) {
  const configPath = path.join(
    getConfigsFolderPath({
      pathToHome,
      chain,
      configsFolder,
      configsFolderSuffix,
    }),
    config,
  );
  if (!fs.existsSync(configPath)) {
    return null;
  }
  return yaml.parse(
    fs.readFileSync(configPath, {
      encoding: "utf-8",
    }),
    {
      customTags: (tags) =>
        tags.filter((tag) => {
          if (typeof tag === "string") {
            return true;
          }
          if (tag.format !== "HEX") {
            return true;
          }
          return false;
        }),
    },
  );
}

export function getConfigPath({
  pathToHome,
  chain,
  configsFolder,
  config,
}: {
  pathToHome: string;
  chain: string;
  configsFolder?: string;
  config: ConfigName;
}) {
  return path.join(
    getConfigsFolderPath({ pathToHome, chain, configsFolder }),
    config,
  );
}

export function getAllConfigsPath({
  pathToHome,
  chain,
  configsFolder,
}: {
  pathToHome: string;
  chain: string;
  configsFolder?: string;
}) {
  const configPaths = {} as Record<ConfigName, string>;
  configNames.forEach((config) => {
    configPaths[config] = getConfigPath({
      pathToHome,
      chain,
      configsFolder,
      config,
    });
  });
  return configPaths;
}

export function getConfigsFolderPath({
  pathToHome,
  chain,
  configsFolder,
  configsFolderSuffix,
}: {
  pathToHome: string;
  chain: string;
  configsFolder?: string;
  configsFolderSuffix?: string;
}) {
  return path.join(
    pathToHome,
    "chains",
    chain,
    configsFolder ?? "configs",
    configsFolderSuffix ?? "",
  );
}

export function replaceL1BatchMinAgeBeforeExecuteSeconds(
  pathToHome: string,
  chain: string,
  value: number,
) {
  const generalConfigPath = getConfigPath({
    pathToHome,
    chain,
    config: "general.yaml",
  });
  const generalConfig = fsSync.readFileSync(generalConfigPath, "utf8");
  const generalConfigObject = yaml.parse(generalConfig);
  if (value == 0) {
    delete generalConfigObject["eth"]["sender"][
      "l1_batch_min_age_before_execute_seconds"
    ];
  } else {
    generalConfigObject["eth"]["sender"][
      "l1_batch_min_age_before_execute_seconds"
    ] = value;
  }
  const newGeneralConfig = yaml.stringify(generalConfigObject);
  fs.writeFileSync(generalConfigPath, newGeneralConfig, "utf8");
}
