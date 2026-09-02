#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const languages = process.env.LANGUAGES || process.argv[2] || 'typescript';

const result = process.platform === 'win32'
  ? spawnSync(
    'powershell',
    [
      '-NoProfile',
      '-ExecutionPolicy',
      'Bypass',
      '-File',
      path.join(__dirname, 'generate-sdk.ps1'),
      '-Languages',
      languages,
    ],
    { stdio: 'inherit' },
  )
  : spawnSync(
    'bash',
    [path.join(__dirname, 'generate-sdk.sh')],
    {
      stdio: 'inherit',
      env: {
        ...process.env,
        LANGUAGES: languages,
      },
    },
  );

process.exit(result.status ?? 1);
