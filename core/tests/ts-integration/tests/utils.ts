import * as fs from 'fs';
import { getConfigPath } from 'utils/build/file-configs';

export function setInternalPubdataPricingMultiplier(pathToHome: string, fileConfig: any, value: number) {
    setPropertyInGeneralConfig(pathToHome, fileConfig, 'internal_pubdata_pricing_multiplier', value);
}

export function setInternalL1PricingMultiplier(pathToHome: string, fileConfig: any, value: number) {
    setPropertyInGeneralConfig(pathToHome, fileConfig, 'internal_l1_pricing_multiplier', value);
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
