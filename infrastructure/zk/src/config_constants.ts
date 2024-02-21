// Include constants to be used on config files.
// Prefix ROLLUP_ or VALIDIUM_ is used to use the right constant depending on the running mode
const VALIDIUM_COMPUTE_OVERHEAD_PART: number = 1.0;
const ROLLUP_COMPUTE_OVERHEAD_PART: number = 0.0;
const VALIDIUM_PUBDATA_OVERHEAD_PART: number = 0.0;
const ROLLUP_PUBDATA_OVERHEAD_PART: number = 1.0;
const VALIDIUM_BATCH_OVERHEAD_L1_GAS: number = 1000000;
const ROLLUP_BATCH_OVERHEAD_L1_GAS: number = 800000;
const VALIDIUM_MAX_PUBDATA_PER_BATCH: number = 1000000000000;
const ROLLUP_MAX_PUBDATA_PER_BATCH: number = 100000;
const VALIDIUM_L1_GAS_PER_PUBDATA_BYTE: number = 0;
const ROLLUP_L1_GAS_PER_PUBDATA_BYTE: number = 17;
const VALIDIUM_L1_BATCH_COMMIT_DATA_GENERATOR_MODE: string = '"Validium"';
const ROLLUP_L1_BATCH_COMMIT_DATA_GENERATOR_MODE: string = '"Rollup"';

//Define config file's path that are updated depending on the running mode (Validium or Rollup)
export const CHAIN_CONFIG_PATH = 'etc/env/base/chain.toml';
export const ETH_SENDER_PATH = 'etc/env/base/eth_sender.toml';

export function getEthSenderConfigConstants(validiumMode: boolean): Record<string, number | string | null> {
    return {
        l1_gas_per_pubdata_byte: validiumMode ? VALIDIUM_L1_GAS_PER_PUBDATA_BYTE : ROLLUP_L1_GAS_PER_PUBDATA_BYTE
    };
}

export function getChainConfigConstants(validiumMode: boolean): Record<string, number | string | null> {
    return {
        compute_overhead_part: validiumMode ? VALIDIUM_COMPUTE_OVERHEAD_PART : ROLLUP_COMPUTE_OVERHEAD_PART,
        pubdata_overhead_part: validiumMode ? VALIDIUM_PUBDATA_OVERHEAD_PART : ROLLUP_PUBDATA_OVERHEAD_PART,
        batch_overhead_l1_gas: validiumMode ? VALIDIUM_BATCH_OVERHEAD_L1_GAS : ROLLUP_BATCH_OVERHEAD_L1_GAS,
        max_pubdata_per_batch: validiumMode ? VALIDIUM_MAX_PUBDATA_PER_BATCH : ROLLUP_MAX_PUBDATA_PER_BATCH,
        l1_batch_commit_data_generator_mode: validiumMode
            ? VALIDIUM_L1_BATCH_COMMIT_DATA_GENERATOR_MODE
            : ROLLUP_L1_BATCH_COMMIT_DATA_GENERATOR_MODE
    };
}
