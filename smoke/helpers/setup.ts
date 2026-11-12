import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';

import { resolveRepositoryRoot } from './binary.js';

if (!process.env.LOTAR_TEST_SILENT) {
    process.env.LOTAR_TEST_SILENT = '1';
}

if (!process.env.LOTAR_BINARY_PATH) {
    // Allow overriding via LOTAR_BIN for backwards compatibility
    if (process.env.LOTAR_BIN) {
        process.env.LOTAR_BINARY_PATH = process.env.LOTAR_BIN;
    }
}

// Config overrides from the caller (an agent's LOTAR_DEFAULT_REPORTER) would
// beat the fixtures' own config (DEV-338). The list comes from
// ENV_OVERRIDE_DEFINITIONS; `npm run smoke:ui` sets LOTAR_WEB_UI_PATH itself.
const KEPT_OVERRIDES = new Set(['LOTAR_WEB_UI_PATH']);
const overridesSource = fs.readFileSync(
    path.join(resolveRepositoryRoot(), '..', 'src/config/env_overrides.rs'),
    'utf8',
);
const definitions = /ENV_OVERRIDE_DEFINITIONS[^=]*=\s*&\[([\s\S]*?)\n\];/.exec(overridesSource)?.[1] ?? '';
const overrideVars = [...definitions.matchAll(/"(LOTAR_[A-Z0-9_]+)"/g)].map(([, name]) => name as string);
if (overrideVars.length === 0) {
    throw new Error('Found no env vars in ENV_OVERRIDE_DEFINITIONS (src/config/env_overrides.rs)');
}
for (const name of overrideVars) {
    if (!KEPT_OVERRIDES.has(name)) {
        Reflect.deleteProperty(process.env, name);
    }
}
