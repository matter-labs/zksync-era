const multivmContractsPath = 'etc/multivm_contracts';
const systemContractsPath = `${multivmContractsPath}/contracts`;
const contractsPath = `${multivmContractsPath}/system-contracts`;

enum ProtocolVersions {
    Version12 = 12,
    Version13,
    Version14,
    Version15,
    Version16
}

const systemContractProtocolVersions = {
    [ProtocolVersions.Version12]: `${systemContractsPath}/vm_1_3_2`,
    [ProtocolVersions.Version13]: `${systemContractsPath}/vm_virtual_blocks`,
    [ProtocolVersions.Version14]: `${systemContractsPath}/vm_virtual_blocks`,
    [ProtocolVersions.Version15]: `${systemContractsPath}/vm_virtual_blocks`,
    [ProtocolVersions.Version16]: `${systemContractsPath}/vm_virtual_blocks`
};

const contractsVersions = {
    [ProtocolVersions.Version12]: `${contractsPath}/vm_1_3_2`,
    [ProtocolVersions.Version13]: `${contractsPath}/vm_virtual_blocks`,
    [ProtocolVersions.Version14]: `${contractsPath}/vm_virtual_blocks`,
    [ProtocolVersions.Version15]: `${contractsPath}/vm_virtual_blocks`,
    [ProtocolVersions.Version16]: `${contractsPath}/vm_virtual_blocks`
};
function systemContractsPathForProtocol(version: ProtocolVersions): string {
    return systemContractProtocolVersions[version];
}

function contractsPathForProtocol(version: ProtocolVersions): string {
    return contractsVersions[version];
}
