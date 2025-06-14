import { Authorizer } from '@/permissions/authorizer';

export const authorizer = new Authorizer();
authorizer.reloadFromEnv();
