import { isolatedExternalNode } from 'zk/build/server';

describe('Dockerized rust binaries runner', () => {
    test('Can spawn functional isolated node', async () => {
        await isolatedExternalNode();
    });
});
