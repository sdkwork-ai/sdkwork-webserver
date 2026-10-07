// Shared helpers for the native installer packagers (webserver-deb.mjs,
// webserver-rpm.mjs). Every export here existed verbatim in BOTH scripts;
// this module is the single copy, not a new abstraction (CODE_STYLE_SPEC §7
// single-source rule for build-critical tooling).
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

export const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

/// Reads the release version from the application manifest.
export function appVersion() {
  const manifest = JSON.parse(
    readFileSync(path.join(REPO_ROOT, 'sdkwork.app.config.json'), 'utf8'),
  );
  const version = manifest?.release?.currentVersion;
  if (typeof version !== 'string' || version.length === 0) {
    throw new Error('sdkwork.app.config.json release.currentVersion is missing');
  }
  return version;
}

/// Bounded spawnSync wrapper: repo-root cwd, 10-minute budget, and a thrown
/// error carrying stderr on failure.
export function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? REPO_ROOT,
    encoding: 'utf8',
    env: options.env ?? process.env,
    stdio: options.capture ? 'pipe' : 'inherit',
    timeout: options.timeoutMs ?? 10 * 60 * 1000,
    maxBuffer: 16 * 1024 * 1024,
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    const detail = result.error?.message ?? result.stderr?.trim() ?? `exit ${result.status}`;
    throw new Error(`${command} ${args.join(' ')} failed: ${detail}`);
  }
  return result;
}

export function archiveBaseName(settings) {
  return (
    `sdkwork-webserver-linux-${settings.architecture}-standalone-server-${settings.version}`
  );
}

export function archivePath(outputRoot, settings) {
  return path.join(outputRoot, `${archiveBaseName(settings)}.tar.gz`);
}

export function wslPath(windowsPath) {
  const match = windowsPath.match(/^([A-Za-z]):\\(.*)$/);
  if (!match) {
    throw new Error(`cannot convert Windows path to WSL: ${windowsPath}`);
  }
  return `/mnt/${match[1].toLowerCase()}/${match[2].replace(/\\/g, '/')}`;
}

export function wslOrNative(filePath) {
  return process.platform === 'win32' ? wslPath(filePath) : filePath;
}

/// Runs a Linux packaging tool, through WSL when invoked from Windows.
export function runWslTool(tool, args, env = {}) {
  if (process.platform === 'win32') {
    const shellArgs = args.map((arg) => (arg.includes(' ') ? `'${arg}'` : arg)).join(' ');
    return run('wsl.exe', ['-d', 'Ubuntu-22.04', '-e', 'bash', '-lc', `${tool} ${shellArgs}`], {
      capture: true,
      env: { ...process.env, ...env },
    });
  }
  return run(tool, args, { capture: true, env: { ...process.env, ...env } });
}

export function sha256File(filePath) {
  const hash = createHash('sha256');
  hash.update(readFileSync(filePath));
  return hash.digest('hex');
}

/// Substitutes `__KEY__` tokens with their values (literal string replace,
/// unordered — a value may itself contain token-like text).
export function renderTemplate(templatePath, values) {
  let text = readFileSync(templatePath, 'utf8');
  for (const [key, value] of Object.entries(values)) {
    text = text.split(`__${key}__`).join(value);
  }
  return text;
}
