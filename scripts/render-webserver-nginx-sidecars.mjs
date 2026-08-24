#!/usr/bin/env node
/**
 * Render deployments/webserver/nginx.<profile>.<environment>.conf sidecars for
 * sdkwork-webserver (delegates to the shared workspace tool).
 */
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { renderModuleNginxSidecars } from '../../sdkwork-specs/tools/webserver/render-nginx-sidecars.mjs';

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

renderModuleNginxSidecars(appRoot, { validate: true });
