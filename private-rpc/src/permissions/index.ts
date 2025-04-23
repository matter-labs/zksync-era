import { readFileSync } from 'node:fs';

import { env } from '@/env';
import { Authorizer } from '@/permissions/authorizer';

const buf = readFileSync(env.PERMISSIONS_YAML_PATH);
export const authorizer = Authorizer.fromBuffer(buf);
