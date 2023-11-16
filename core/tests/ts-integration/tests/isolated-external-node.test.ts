import {isolatedExternalNode} from "zk/build/server";
import * as utils from 'zk/build/utils';


describe('ETH token checks', () => {


    test('Can spawn functional isolated node', async () => {
            await utils.spawn(' cargo build --release');
            await utils.spawn(' zk docker build integration-test-node');
            await isolatedExternalNode();
        }
    );
});
