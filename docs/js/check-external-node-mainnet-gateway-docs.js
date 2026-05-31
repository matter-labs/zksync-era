const fs = require("fs");
const path = require("path");
const assert = require("assert");

const repoRoot = path.resolve(__dirname, "..", "..");
const mainnetComposePath = path.join(
    repoRoot,
    "docs",
    "src",
    "guides",
    "external-node",
    "docker-compose-examples",
    "mainnet-external-node-docker-compose.yml"
);
const mainnetConfigPath = path.join(
    repoRoot,
    "docs",
    "src",
    "guides",
    "external-node",
    "prepared_configs",
    "mainnet-config.env"
);
const otherChainsGuidePath = path.join(
    repoRoot,
    "docs",
    "src",
    "guides",
    "external-node",
    "11_setup_for_other_chains.md"
);

const mainnetCompose = fs.readFileSync(mainnetComposePath, "utf8");
const mainnetConfig = fs.readFileSync(mainnetConfigPath, "utf8");
const otherChainsGuide = fs.readFileSync(otherChainsGuidePath, "utf8");

assert(
    !mainnetCompose.includes("EN_GATEWAY_URL"),
    "mainnet docker-compose example should not set EN_GATEWAY_URL"
);
assert(
    !mainnetConfig.includes("EN_GATEWAY_URL"),
    "mainnet prepared config should not set EN_GATEWAY_URL"
);
assert(
    otherChainsGuide.includes(
        "Set `EN_GATEWAY_URL` only if the chain operator gave you a reachable Gateway RPC endpoint for that chain."
    ),
    "other-chains guide should explain when EN_GATEWAY_URL is actually required"
);
assert(
    otherChainsGuide.includes(
        "For ZKsync Era mainnet, the default external-node setup does not need an explicit `EN_GATEWAY_URL`."
    ),
    "other-chains guide should state that Era mainnet does not need an explicit EN_GATEWAY_URL"
);
