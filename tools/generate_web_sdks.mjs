#!/usr/bin/env node

import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const WORKSPACE_ROOT = path.resolve(REPO_ROOT, '..');
const GENERATOR_PATH = path.join(WORKSPACE_ROOT, 'sdkwork-sdk-generator', 'bin', 'sdkgen.js');
const SDK_ROOT = path.join(REPO_ROOT, 'sdks');
const FAMILY_NAMES = [
  'sdkwork-webserver-app-sdk',
  'sdkwork-webserver-backend-sdk',
  'sdkwork-webserver-internal-sdk',
];

function readJson(filePath) {
  return JSON.parse(readFileSync(filePath, 'utf8').replace(/^\uFEFF/u, ''));
}

function namespaceFor(language, sdkName, packageName) {
  const surfaceParts = sdkName
    .replace(/^sdkwork-/u, '')
    .replace(/-sdk$/u, '')
    .split('-');
  if (language === 'java' || language === 'kotlin') {
    return `com.sdkwork.${surfaceParts.join('.')}.sdk`;
  }
  if (language === 'csharp') return packageName;
  if (language === 'php') {
    const [domain, ...surface] = surfaceParts;
    const pascal = (value) => `${value.slice(0, 1).toUpperCase()}${value.slice(1)}`;
    return `SDKWork\\${pascal(domain)}\\${surface.map(pascal).join('')}Sdk`;
  }
  return null;
}

function packageNameFor(manifest, language) {
  return language.consumerPackageName ?? language.name ?? manifest.packageName;
}

export function collectGenerationPlans({ familyName } = {}) {
  const selectedFamilies = familyName ? [familyName] : FAMILY_NAMES;
  for (const selected of selectedFamilies) {
    if (!FAMILY_NAMES.includes(selected)) throw new Error(`unsupported SDK family: ${selected}`);
  }

  return selectedFamilies.flatMap((selected) => {
    const familyRoot = path.join(SDK_ROOT, selected);
    const manifestPath = path.join(familyRoot, 'sdk-manifest.json');
    const manifest = readJson(manifestPath);
    const input = path.resolve(familyRoot, manifest.generationInputSpec);
    const languages = (manifest.languages ?? []).filter(
      (language) => language.generationState === 'materialized',
    );
    if (languages.length === 0) throw new Error(`${selected} declares no materialized languages`);

    return languages.map((language) => {
      let packageName = packageNameFor(manifest, language);
      if (typeof packageName !== 'string' || packageName.length === 0) {
        throw new Error(`${selected}/${language.language} does not declare a package name`);
      }
      // Rust transport SDKs follow the workspace alias convention: the generated
      // crate carries the "-generated-rust" suffix so the root workspace can
      // alias it via `package = "...-generated-rust"` (same pattern as the
      // sdkwork-drive / sdkwork-knowledgebase internal SDKs).
      if (language.language === 'rust') packageName = `${packageName}-generated-rust`;
      return {
        apiPrefix: manifest.discoverySurface?.apiPrefix ?? '',
        familyRoot,
        input,
        language: language.language,
        namespace: namespaceFor(language.language, manifest.sdkName, packageName),
        output: path.resolve(familyRoot, language.generatedPath),
        packageName,
        sdkName: manifest.sdkName,
        sdkType: manifest.sdkType,
        version: language.version ?? manifest.apiVersion,
      };
    });
  });
}

function assertPlanPaths(plan) {  if (!existsSync(GENERATOR_PATH)) throw new Error(`canonical SDK generator not found: ${GENERATOR_PATH}`);
  if (!existsSync(plan.input)) throw new Error(`SDK generation input not found: ${plan.input}`);
  const familyPrefix = `${path.resolve(plan.familyRoot)}${path.sep}`;
  if (!path.resolve(plan.output).startsWith(familyPrefix)) {
    throw new Error(`refusing SDK output outside family root: ${plan.output}`);
  }
}

function generatorArgs(plan, check) {
  const args = [
    GENERATOR_PATH,
    'generate',
    '-i', plan.input,
    '-o', plan.output,
    '-n', plan.sdkName,
    '-t', plan.sdkType,
    '-l', plan.language,
    '--fixed-sdk-version', plan.version,
    '--base-url', 'http://localhost:3800',
    '--api-prefix', plan.apiPrefix,
    '--package-name', plan.packageName,
    '--standard-profile', 'sdkwork-v3',
    '--sdk-root', plan.familyRoot,
    '--sdk-name', plan.sdkName,
    '--no-sync-published-version',
    '--json',
  ];
  if (plan.namespace) args.push('--namespace', plan.namespace);
  if (check) args.push('--dry-run');
  return args;
}

function runPlan(plan, check) {
  assertPlanPaths(plan);
  const result = spawnSync(process.execPath, generatorArgs(plan, check), {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    windowsHide: true,
  });
  if (result.status !== 0) {
    const detail = result.stderr.trim() || result.stdout.trim() || `exit ${result.status ?? 1}`;
    throw new Error(`${plan.sdkName}/${plan.language} generation failed: ${detail}`);
  }
  const report = JSON.parse(result.stdout);
  const label = `${plan.sdkName}/${plan.language}`;
  if (check && report.hasChanges) {
    const changes = report.syncSummary?.changes ?? {};
    const paths = [
      ...(changes.createdGeneratedFiles ?? []),
      ...(changes.updatedGeneratedFiles ?? []),
      ...(changes.deletedGeneratedFiles ?? []),
      ...(changes.scaffoldedFiles ?? []),
    ];
    throw new Error(`${label} generated output drift: ${paths.join(', ') || 'changes detected'}`);
  }
  console.log(`[sdkwork-web-sdk] ${check ? 'current' : 'generated'} ${label}`);
}

function option(argv, name) {
  const index = argv.indexOf(name);
  return index >= 0 ? argv[index + 1] : undefined;
}

export function main(argv = process.argv.slice(2)) {
  const check = argv.includes('--check');
  const familyName = option(argv, '--family');
  const plans = collectGenerationPlans({ familyName });
  const failures = [];
  for (const plan of plans) {
    try {
      runPlan(plan, check);
    } catch (error) {
      failures.push(error instanceof Error ? error.message : String(error));
    }
  }
  if (failures.length > 0) {
    for (const failure of failures) console.error(`[sdkwork-web-sdk] ${failure}`);
    process.exitCode = 1;
    return;
  }
  console.log(`[sdkwork-web-sdk] ${check ? 'verified' : 'generated'} ${plans.length} materialized targets`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
