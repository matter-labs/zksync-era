import * as bigNumberMatchers from '../matchers/big-number';
import * as ethPrimitives from '../matchers/eth-primitives';
import * as transaction from '../matchers/transaction';
import * as fail from '../matchers/fail';

expect.extend(bigNumberMatchers);
expect.extend(ethPrimitives);
expect.extend(transaction);
expect.extend(fail);
