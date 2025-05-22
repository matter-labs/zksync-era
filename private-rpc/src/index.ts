import closeWithGrace from 'close-with-grace';

import { db } from '@/db';
import { authorizer } from '@/permissions';
import { buildApp } from './build-app';
import { env } from './env';

const app = buildApp(true, db, env.TARGET_RPC, authorizer, env.CREATE_TOKEN_SECRET, env.CORS_ORIGIN);

closeWithGrace(async ({ signal, err }) => {
    if (err) {
        app.log.error({ err }, 'server closing with error');
    } else {
        app.log.info(`${signal} received, server closing`);
    }
    await app.close();
});

await app.listen({ host: '0.0.0.0', port: env.PORT });
