#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';
import { stringify as stringifyYaml } from 'yaml';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const SOURCE_ROOT = path.join(REPO_ROOT, 'deployments', 'kubernetes');
const OUTPUT_ROOT = path.join(REPO_ROOT, '.sdkwork', 'runtime', 'kubernetes');
const IMAGE_DIGEST_PLACEHOLDER = '__SDKWORK_IMAGE_DIGEST__';
const WEBSITE_TENANT_FLEET_NAME_PLACEHOLDER =
  '__SDKWORK_WEBSITE_TENANT_FLEET_NAME__';
const WEBSITE_NODE_NAME_PLACEHOLDER = '__SDKWORK_WEBSITE_NODE_NAME__';
const WEBSITE_NODE_SECRET_NAME_PLACEHOLDER = '__SDKWORK_WEBSITE_NODE_SECRET_NAME__';
const WEBSITE_CONFIG_MAP_NAME_PLACEHOLDER = '__SDKWORK_WEBSITE_CONFIG_MAP_NAME__';
const WEBSITE_HOST_CONFIG_SOURCE = path.join(
  REPO_ROOT,
  'etc',
  'data-plane',
  'website.cloud.config.json',
);
const WEBSITE_HOST_CONFIG_MAX_BYTES = 1024 * 1024;
const WEBSITE_HOST_CONFIG_KEY = 'sdkwork.webserver.config.json';
const WEBSITE_CONFIG_MAP_MANIFEST = 'config-map.yaml';
const MANIFESTS = [
  'migration-job.yaml',
  'deployment.yaml',
  'service.yaml',
  'network-policy.yaml',
];

function readOptionValue(argv, index, option) {
  const value = argv[index + 1];
  if (value === undefined || value.startsWith('--')) {
    throw new Error(`${option} requires a value`);
  }
  return value;
}

function setSingleOption(settings, key, option, value) {
  if (Object.hasOwn(settings, key)) {
    throw new Error(`${option} cannot be provided more than once`);
  }
  settings[key] = value;
}

function parseArgs(argv) {
  const settings = {};
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--image-digest') {
      setSingleOption(
        settings,
        'imageDigest',
        argument,
        readOptionValue(argv, index, argument),
      );
      index += 1;
    } else if (argument === '--website-tenant-fleet-name') {
      setSingleOption(
        settings,
        'websiteTenantFleetName',
        argument,
        readOptionValue(argv, index, argument),
      );
      index += 1;
    } else if (argument === '--website-node-name') {
      setSingleOption(
        settings,
        'websiteNodeName',
        argument,
        readOptionValue(argv, index, argument),
      );
      index += 1;
    } else if (argument === '--website-node-secret-name') {
      setSingleOption(
        settings,
        'websiteNodeSecretName',
        argument,
        readOptionValue(argv, index, argument),
      );
      index += 1;
    } else if (argument === '--website-trusted-proxy-cidr') {
      settings.websiteTrustedProxyCidrs ??= [];
      settings.websiteTrustedProxyCidrs.push(readOptionValue(argv, index, argument));
      index += 1;
    } else if (argument === '--help' || argument === '-h') {
      settings.help = true;
    } else {
      throw new Error(`unsupported option: ${argument}`);
    }
  }
  return settings;
}

function normalizeDnsLabel(value, option, maxLength = 63) {
  const label = value?.trim().toLowerCase();
  if (
    label?.length > maxLength ||
    !/^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/u.test(label ?? '')
  ) {
    throw new Error(
      `${option} must be a lowercase Kubernetes DNS label of at most ${maxLength} characters`,
    );
  }
  return label;
}

function normalizeTenantFleetName(value) {
  const label = value?.trim().toLowerCase();
  if (!/^tf-[a-z2-7]{15}$/u.test(label ?? '')) {
    throw new Error(
      '--website-tenant-fleet-name must be tf- followed by exactly 15 lowercase base32 characters',
    );
  }
  return label;
}

function normalizeDigest(value) {
  const digest = value?.trim().replace(/^sha256:/u, '').toLowerCase();
  if (!/^[a-f0-9]{64}$/u.test(digest ?? '')) {
    throw new Error('--image-digest must be a sha256 digest containing exactly 64 hex characters');
  }
  return digest;
}

function assertRegularFile(filePath, label) {
  if (!existsSync(filePath)) {
    throw new Error(`${label} is missing`);
  }
  const stat = lstatSync(filePath);
  if (!stat.isFile() || stat.isSymbolicLink()) {
    throw new Error(`${label} must be a regular non-symlink file`);
  }
  return stat;
}

function normalizeTrustedProxyCidrs(values) {
  if (!Array.isArray(values) || values.length === 0) {
    throw new Error('--website-trusted-proxy-cidr must be provided at least once');
  }
  if (values.length > 64) {
    throw new Error('--website-trusted-proxy-cidr cannot be provided more than 64 times');
  }
  const cidrs = values.map((value) => {
    const cidr = value?.trim().toLowerCase();
    if (!cidr || cidr.length > 128 || /[\u0000-\u001f\u007f]/u.test(cidr)) {
      throw new Error('--website-trusted-proxy-cidr contains an invalid value');
    }
    if (cidr === '0.0.0.0/0' || cidr === '::/0') {
      throw new Error('universal trusted proxy CIDRs are forbidden');
    }
    return cidr;
  });
  if (new Set(cidrs).size !== cidrs.length) {
    throw new Error('--website-trusted-proxy-cidr values must be unique');
  }
  return cidrs;
}

function materializeWebsiteHostConfig(trustedProxyCidrs) {
  const stat = assertRegularFile(WEBSITE_HOST_CONFIG_SOURCE, 'website host config source');
  if (stat.size === 0 || stat.size > WEBSITE_HOST_CONFIG_MAX_BYTES) {
    throw new Error('website host config source must be non-empty and no larger than 1 MiB');
  }
  const config = JSON.parse(readFileSync(WEBSITE_HOST_CONFIG_SOURCE, 'utf8'));
  if (
    config?.appKey !== 'sdkwork-website-delivery' ||
    !Array.isArray(config.listeners) ||
    config.listeners.length !== 1
  ) {
    throw new Error('website host config source has an invalid application identity');
  }
  const listener = config.listeners.find((candidate) => candidate?.id === 'website-http');
  if (
    !listener ||
    listener.bind !== '0.0.0.0' ||
    listener.port !== 8080 ||
    !Array.isArray(listener.protocols) ||
    listener.protocols.length !== 1 ||
    listener.protocols[0] !== 'http1'
  ) {
    throw new Error('website host config source must contain the cloud website-http listener');
  }
  for (const collection of ['certificates', 'tlsPolicies', 'resolvers', 'upstreams']) {
    if (!Array.isArray(config[collection]) || config[collection].length !== 0) {
      throw new Error(`website host config source ${collection} must remain empty`);
    }
  }
  if (
    !Array.isArray(config.resources) ||
    config.resources.length !== 1 ||
    config.resources[0]?.type !== 'respond'
  ) {
    throw new Error('website host config source may contain only the unmatched response resource');
  }
  listener.trustedProxy = {
    trustedCidrs: trustedProxyCidrs,
    header: 'x-forwarded-for',
    recursive: true,
    maxHops: 16,
    maxHeaderBytes: 4096,
  };
  return `${JSON.stringify(config, null, 2)}\n`;
}

function validateWebsiteHostConfig(configPath) {
  const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
  const validation = spawnSync(
    cargo,
    [
      'run',
      '--quiet',
      '--offline',
      '-p',
      'sdkwork-webserver-website-delivery-edge-runtime',
      '--',
      'validate',
      configPath,
    ],
    {
      cwd: REPO_ROOT,
      encoding: 'utf8',
      windowsHide: true,
      maxBuffer: 1024 * 1024,
    },
  );
  if (validation.error) {
    throw new Error(`website host config compiler could not start: ${validation.error.message}`);
  }
  if (validation.status !== 0) {
    const diagnostic = validation.stderr.trim().slice(0, 4096);
    throw new Error(
      `website host config compiler rejected the config${diagnostic ? `: ${diagnostic}` : ''}`,
    );
  }
}

function renderWebsiteConfigMap(settings, config, outputDirectory) {
  const configMap = {
    apiVersion: 'v1',
    kind: 'ConfigMap',
    metadata: {
      name: settings.websiteConfigMapName,
      labels: {
        'app.kubernetes.io/name': 'sdkwork-webserver',
        'app.kubernetes.io/component': 'website-data-plane',
        'app.kubernetes.io/part-of': 'sdkwork-webserver',
        'sdkwork.com/tenant-fleet': settings.websiteTenantFleetName,
        'sdkwork.com/web-node': settings.websiteNodeName,
        'sdkwork.com/config-revision': settings.websiteConfigRevision.slice(0, 16),
      },
      annotations: {
        'sdkwork.com/config-sha256': settings.websiteConfigRevision,
      },
    },
    immutable: true,
    data: {
      [WEBSITE_HOST_CONFIG_KEY]: config,
    },
  };
  writeFileSync(
    path.join(outputDirectory, WEBSITE_CONFIG_MAP_MANIFEST),
    stringifyYaml(configMap, { lineWidth: 0 }),
    { encoding: 'utf8', flag: 'wx', mode: 0o600 },
  );
}

function renderManifest(name, settings, outputDirectory) {
  const source = path.join(SOURCE_ROOT, name);
  assertRegularFile(source, `Kubernetes source ${name}`);
  const authored = readFileSync(source, 'utf8');
  const occurrenceCount = authored.split(IMAGE_DIGEST_PLACEHOLDER).length - 1;
  const expectedCount =
    name === 'migration-job.yaml' ? 1 : name === 'deployment.yaml' ? 2 : 0;
  if (occurrenceCount !== expectedCount) {
    throw new Error(`${name} must contain exactly ${expectedCount} image digest placeholder(s)`);
  }
  if (/:latest(?:\s|$)/u.test(authored)) {
    throw new Error(`${name} must not contain a latest image tag`);
  }
  const nodeNameCount = authored.split(WEBSITE_NODE_NAME_PLACEHOLDER).length - 1;
  const tenantFleetNameCount =
    authored.split(WEBSITE_TENANT_FLEET_NAME_PLACEHOLDER).length - 1;
  const nodeSecretNameCount =
    authored.split(WEBSITE_NODE_SECRET_NAME_PLACEHOLDER).length - 1;
  const configMapNameCount = authored.split(WEBSITE_CONFIG_MAP_NAME_PLACEHOLDER).length - 1;
  const expectsNodeName = name === 'deployment.yaml' || name === 'service.yaml';
  const expectsNodeCredentials = name === 'deployment.yaml';
  const expectsTenantFleet =
    name === 'deployment.yaml' || name === 'service.yaml' || name === 'network-policy.yaml';
  if (
    (expectsNodeName && nodeNameCount === 0) ||
    (!expectsNodeName && nodeNameCount !== 0) ||
    (expectsNodeCredentials && (nodeSecretNameCount === 0 || configMapNameCount === 0)) ||
    (!expectsNodeCredentials && (nodeSecretNameCount !== 0 || configMapNameCount !== 0))
  ) {
    throw new Error(`${name} has an invalid website Node placeholder contract`);
  }
  if (
    (expectsTenantFleet && tenantFleetNameCount === 0) ||
    (!expectsTenantFleet && tenantFleetNameCount !== 0)
  ) {
    throw new Error(`${name} has an invalid website tenant fleet placeholder contract`);
  }
  const rendered = authored
    .replaceAll(IMAGE_DIGEST_PLACEHOLDER, settings.imageDigest)
    .replaceAll(
      WEBSITE_TENANT_FLEET_NAME_PLACEHOLDER,
      settings.websiteTenantFleetName,
    )
    .replaceAll(WEBSITE_NODE_NAME_PLACEHOLDER, settings.websiteNodeName)
    .replaceAll(WEBSITE_NODE_SECRET_NAME_PLACEHOLDER, settings.websiteNodeSecretName)
    .replaceAll(WEBSITE_CONFIG_MAP_NAME_PLACEHOLDER, settings.websiteConfigMapName);
  const output = path.join(outputDirectory, name);
  writeFileSync(output, rendered, { encoding: 'utf8', flag: 'wx', mode: 0o600 });
}

function main() {
  const settings = parseArgs(process.argv.slice(2));
  if (settings.help) {
    console.log(
      'Usage: node scripts/render-kubernetes-manifests.mjs --image-digest <sha256> --website-tenant-fleet-name <tf-[a-z2-7]{15}> --website-node-name <dns-label> --website-node-secret-name <dns-label> --website-trusted-proxy-cidr <cidr> [--website-trusted-proxy-cidr <cidr> ...]',
    );
    return;
  }
  settings.imageDigest = normalizeDigest(settings.imageDigest);
  settings.websiteTenantFleetName = normalizeTenantFleetName(
    settings.websiteTenantFleetName,
  );
  settings.websiteNodeName = normalizeDnsLabel(
    settings.websiteNodeName,
    '--website-node-name',
    24,
  );
  settings.websiteNodeSecretName = normalizeDnsLabel(
    settings.websiteNodeSecretName,
    '--website-node-secret-name',
  );
  settings.websiteTrustedProxyCidrs = normalizeTrustedProxyCidrs(
    settings.websiteTrustedProxyCidrs,
  );
  const outputDirectory = path.join(
    OUTPUT_ROOT,
    `${settings.imageDigest.slice(0, 16)}-${settings.websiteTenantFleetName}-${settings.websiteNodeName}`,
  );
  if (existsSync(outputDirectory)) {
    throw new Error(`render output already exists: ${path.relative(REPO_ROOT, outputDirectory)}`);
  }
  const stagingDirectory = `${outputDirectory}.tmp-${process.pid}`;
  rmSync(stagingDirectory, { recursive: true, force: true });
  mkdirSync(stagingDirectory, { recursive: true, mode: 0o700 });
  try {
    const hostConfig = materializeWebsiteHostConfig(settings.websiteTrustedProxyCidrs);
    settings.websiteConfigRevision = createHash('sha256').update(hostConfig).digest('hex');
    settings.websiteConfigMapName =
      `sdkwork-webserver-website-config-${settings.websiteTenantFleetName}-${settings.websiteNodeName}-${settings.websiteConfigRevision.slice(0, 16)}`;
    const hostConfigPath = path.join(stagingDirectory, WEBSITE_HOST_CONFIG_KEY);
    writeFileSync(hostConfigPath, hostConfig, { encoding: 'utf8', flag: 'wx', mode: 0o600 });
    validateWebsiteHostConfig(hostConfigPath);
    rmSync(hostConfigPath, { force: true });
    for (const manifest of MANIFESTS) {
      renderManifest(manifest, settings, stagingDirectory);
    }
    renderWebsiteConfigMap(settings, hostConfig, stagingDirectory);
    renameSync(stagingDirectory, outputDirectory);
  } catch (error) {
    rmSync(stagingDirectory, { recursive: true, force: true });
    throw error;
  }
  console.log(path.relative(REPO_ROOT, outputDirectory));
}

try {
  main();
} catch (error) {
  process.stderr.write(`[sdkwork-webserver-kubernetes-render] ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
}
