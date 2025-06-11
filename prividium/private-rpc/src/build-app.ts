import Fastify from 'fastify';
import {
  serializerCompiler,
  validatorCompiler,
  ZodTypeProvider,
} from 'fastify-type-provider-zod';
import { DB } from '@/db';
import { Authorizer } from '@/permissions/authorizer';
import { rpcRoutes } from '@/routes/rpc-routes';
import { usersRoutes } from '@/routes/users-routes';
import cors from '@fastify/cors';
import {healthRoutes} from "@/routes/health-routes";

export function buildApp(
  produceLogs = true,
  db: DB,
  targetRpc: string,
  authorizer: Authorizer,
  createTokenSecret: string,
  corsOrigin: string[],
) {
  const app = Fastify({
    logger: produceLogs,
  }).withTypeProvider<ZodTypeProvider>();

  app.register(cors, {
    origin: corsOrigin,
    credentials: true,
  });

  app.setValidatorCompiler(validatorCompiler);
  app.setSerializerCompiler(serializerCompiler);

  app.decorate('context', {
    db,
    targetRpc,
    authorizer,
    createTokenSecret,
  });

  app.register(usersRoutes, { prefix: '/users' });
  app.register(healthRoutes, { prefix: '/health' });
  app.register(rpcRoutes, { prefix: '/rpc' });

  return app;
}

declare module 'fastify' {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars -- This allow us to have conf available globally.
  interface FastifyInstance {
    context: {
      db: DB;
      targetRpc: string;
      authorizer: Authorizer;
      createTokenSecret: string;
    };
  }
}

export type WebServer = ReturnType<typeof buildApp>;
