import * as fs from 'fs';
import { getConfigPath } from 'utils/build/file-configs';

export function setInternalEnforcedPubdataPrice(pathToHome: string, fileConfig: any, value: number) {
    setGasAdjusterProperty(pathToHome, fileConfig, 'internal_enforced_pubdata_price', value);
}

export function setInternalEnforcedL1GasPrice(pathToHome: string, fileConfig: any, value: number) {
    setGasAdjusterProperty(pathToHome, fileConfig, 'internal_enforced_l1_gas_price', value);
}

export function deleteInternalEnforcedPubdataPrice(pathToHome: string, fileConfig: any) {
    deleteProperty(pathToHome, fileConfig, 'internal_enforced_pubdata_price');
}

export function deleteInternalEnforcedL1GasPrice(pathToHome: string, fileConfig: any) {
    deleteProperty(pathToHome, fileConfig, 'internal_enforced_l1_gas_price');
}

export function setTransactionSlots(pathToHome: string, fileConfig: any, value: number) {
    setPropertyInGeneralConfig(pathToHome, fileConfig, 'transaction_slots', value);
}

function setPropertyInGeneralConfig(pathToHome: string, fileConfig: any, property: string, value: number) {
    const generalConfigPath = getConfigPath({
        pathToHome,
        chain: fileConfig.chain,
        configsFolder: 'configs',
        config: 'general.yaml'
    });
    const generalConfig = fs.readFileSync(generalConfigPath, 'utf8');

    const regex = new RegExp(`${property}:\\s*\\d+(\\.\\d+)?`, 'g');
    const newGeneralConfig = generalConfig.replace(regex, `${property}: ${value}`);

    fs.writeFileSync(generalConfigPath, newGeneralConfig, 'utf8');
}

function setGasAdjusterProperty(pathToHome: string, fileConfig: any, property: string, value: number) {
    const generalConfigPath = getConfigPath({
        pathToHome,
        chain: fileConfig.chain,
        configsFolder: 'configs',
        config: 'general.yaml'
    });
    const generalConfig = fs.readFileSync(generalConfigPath, 'utf8');

    // Define the regex pattern to check if the property already exists
    const propertyRegex = new RegExp(`(^\\s*${property}:\\s*\\d+(\\.\\d+)?$)`, 'm');
    const gasAdjusterRegex = new RegExp('(^\\s*gas_adjuster:.*$)', 'gm');

    let newGeneralConfig;

    if (propertyRegex.test(generalConfig)) {
        // If the property exists, modify its value
        newGeneralConfig = generalConfig.replace(propertyRegex, `    ${property}: ${value}`);
    } else {
        // If the property does not exist, add it under the gas_adjuster section
        newGeneralConfig = generalConfig.replace(gasAdjusterRegex, `$1\n    ${property}: ${value}`);
    }

    fs.writeFileSync(generalConfigPath, newGeneralConfig, 'utf8');
}

function deleteProperty(pathToHome: string, fileConfig: any, property: string) {
    const generalConfigPath = getConfigPath({
        pathToHome,
        chain: fileConfig.chain,
        configsFolder: 'configs',
        config: 'general.yaml'
    });
    const generalConfig = fs.readFileSync(generalConfigPath, 'utf8');

    // Define the regex pattern to find the property line and remove it completely
    const propertyRegex = new RegExp(`^\\s*${property}:.*\\n?`, 'm');

    // Remove the line if the property exists
    const newGeneralConfig = generalConfig.replace(propertyRegex, '');

    fs.writeFileSync(generalConfigPath, newGeneralConfig, 'utf8');
}
