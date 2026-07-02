import NodeEnvironment from 'jest-environment-node';
import type { EnvironmentContext, JestEnvironmentConfig } from '@jest/environment';

// jest-worker uses `JSON.stringify` for parent/worker IPC, and BigInt has no
// default `toJSON`. When a test throws an Error containing a bigint (e.g. an
// ethers `CALL_EXCEPTION` carrying `gasLimit`/`value`), serialization fails
// and the actual error is replaced with `TypeError: Do not know how to
// serialize a BigInt` — masking the real failure.
(BigInt.prototype as unknown as { toJSON: () => string }).toJSON = function () {
    return this.toString();
};

export default class IntegrationTestEnvironment extends NodeEnvironment {
    constructor(config: JestEnvironmentConfig, context: EnvironmentContext) {
        super(config, context);
    }

    override async setup() {
        await super.setup();
        // Provide access to raw console in order to produce less cluttered debug messages
        this.global.rawWriteToConsole = console.log;
    }
}
