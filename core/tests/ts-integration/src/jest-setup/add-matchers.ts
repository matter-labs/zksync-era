import * as ethPrimitives from '../matchers/eth-primitives';
import * as transaction from '../matchers/transaction';
import * as fail from '../matchers/fail';

expect.extend(ethPrimitives);
expect.extend(transaction);
expect.extend(fail);


beforeAll(() => {
    // Add custom serializer for BigInt
    expect.addSnapshotSerializer({
        test: (val) => typeof val === 'bigint',
        // @ts-ignore
        print: (val) => `${val.toString()}n`
    });
});
