import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { create } from 'tar';
import { parse as parseYaml, parseAllDocuments } from 'yaml';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const OUTPUT_ROOT = path.join(REPO_ROOT, 'dist', 'release');
const PACKAGE_FILES = new Map([
  ['bin/sdkwork-api-webserver-standalone-gateway', 'gateway fixture\n'],
  [
    'bin/sdkwork-webserver-website-delivery-edge-runtime',
    'website delivery edge runtime fixture\n',
  ],
  ['bin/sdkwork-web-node-daemon', 'canonical node daemon fixture\n'],
  ['bin/sdkwork-web-agent', 'node daemon fixture\n'],
  ['bin/sdkwork-webserver-certificate-worker', 'certificate worker fixture\n'],
  ['sdkwork.app.config.json', '{}\n'],
  ['specs/iam.module.manifest.json', '{}\n'],
  ['specs/sdkwork.webserver.config.schema.json', '{}\n'],
  ['etc/examples/sdkwork.webserver.config.json', '{}\n'],
  ['etc/examples/public/index.html', '<h1>release fixture</h1>\n'],
  ['etc/data-plane/website.cloud.config.json', '{}\n'],
  ['etc/node-daemon/development.env.example', 'SDKWORK_WEBSERVER_NODE_TOKEN=\n'],
  ['database/README.md', '# Database\n'],
  ['database/database.manifest.json', '{}\n'],
  ['database/contract/prefix-registry.json', '{}\n'],
  ['database/contract/schema.yaml', 'schemaVersion: 1\n'],
  ['database/contract/table-registry.json', '{}\n'],
  ['database/ddl/baseline/postgres/0001_web_baseline.sql', '-- postgres baseline\n'],
  ['database/drift/policy.yaml', 'schemaVersion: 1\n'],
  ['database/seeds/seed.manifest.json', '{}\n'],
  ['database/seeds/common/001_bootstrap.sql', '-- common seed\n'],
]);
const STANDALONE_PC_FILES = new Map([
  [
    'share/sdkwork/webserver-pc/index.html',
    '<!doctype html><html><body><div id="root"></div></body></html>\n',
  ],
  [
    'share/sdkwork/webserver-pc/runtime-env.json',
    `${JSON.stringify(
      {
        environment: 'production',
        deploymentProfile: 'standalone',
        profileId: 'standalone.production',
        runtimeTarget: 'browser',
        browserOriginMode: 'same-origin',
        appApiBaseUrl: '/',
        backendApiBaseUrl: '/',
        driveAppApiBaseUrl: '/',
        appbaseAppApiBaseUrl: '/',
      },
      null,
      2,
    )}\n`,
  ],
  ['share/sdkwork/webserver-pc/assets/index-AbCd1234.js', 'console.log("fixture");\n'],
]);
const STANDALONE_DEPENDENCY_FILES = new Map([
  ['share/sdkwork/iam/database/database.manifest.json', '{}\n'],
  ['share/sdkwork/iam/iam/registry/iam-registry.config.json', '{}\n'],
  ['share/sdkwork/iam/iam/modules/iam-kernel/iam.module.manifest.json', '{}\n'],
  ['share/sdkwork/drive/database/database.manifest.json', '{}\n'],
]);

function packageFilesFor(profile, options = {}) {
  const files = new Map(PACKAGE_FILES);
  if (profile === 'standalone' || options.includePcAssets) {
    for (const [relativePath, text] of STANDALONE_PC_FILES) {
      files.set(relativePath, options.fileOverrides?.get(relativePath) ?? text);
    }
  }
  if (profile === 'standalone' || options.includeDependencyAssets) {
    for (const [relativePath, text] of STANDALONE_DEPENDENCY_FILES) {
      files.set(relativePath, options.fileOverrides?.get(relativePath) ?? text);
    }
  }
  for (const relativePath of options.omitFiles ?? []) {
    files.delete(relativePath);
  }
  return files;
}

function archiveDirectoriesFor(packageFiles) {
  return Array.from(
    new Set(
      [...packageFiles.keys()].flatMap((contentPath) => {
        const segments = contentPath.split('/');
        return segments.slice(0, -1).map((_, index) =>
          ['sdkwork-web', ...segments.slice(0, index + 1)].join('/'),
        );
      }).concat('sdkwork-web'),
    ),
  ).sort();
}

function expectedArchiveEntries(profile) {
  const files = packageFilesFor(profile);
  return archiveDirectoriesFor(files).length + files.size + 1;
}

function sha256(value) {
  return createHash('sha256').update(value).digest('hex');
}

function runValidator(profile, version, architecture = 'x64') {
  const env = { ...process.env };
  delete env.SDKWORK_PACKAGE_VERSION;
  delete env.SDKWORK_RELEASE_VERSION;
  delete env.SDKWORK_PACKAGE_ARCHITECTURE;
  return spawnSync(
    process.execPath,
    [
      'scripts/webserver-release.mjs',
      'validate',
      '--deployment-profile',
      profile,
      '--architecture',
      architecture,
      '--version',
      version,
    ],
    { cwd: REPO_ROOT, encoding: 'utf8', env, windowsHide: true },
  );
}

function runSbom(operation, profile, version, architecture = 'x64') {
  const env = { ...process.env };
  delete env.SDKWORK_PACKAGE_VERSION;
  delete env.SDKWORK_RELEASE_VERSION;
  delete env.SDKWORK_PACKAGE_ARCHITECTURE;
  return spawnSync(
    process.execPath,
    [
      'scripts/webserver-sbom.mjs',
      operation,
      '--deployment-profile',
      profile,
      '--architecture',
      architecture,
      '--version',
      version,
    ],
    { cwd: REPO_ROOT, encoding: 'utf8', env, windowsHide: true },
  );
}

test('Kubernetes renderer binds one tenant fleet and Node identity without cross-fleet selectors', () => {
  const digest = 'ab'.repeat(32);
  const tenantFleetName = 'tf-abcde23456fghij';
  const nodeName = `contract-node-${process.pid}`;
  const secretName = `contract-node-secret-${process.pid}`;
  const outputDirectory = path.join(
    REPO_ROOT,
    '.sdkwork',
    'runtime',
    'kubernetes',
    `${digest.slice(0, 16)}-${tenantFleetName}-${nodeName}`,
  );
  rmSync(outputDirectory, { recursive: true, force: true });
  try {
    const rendered = spawnSync(
      process.execPath,
      [
        'scripts/render-kubernetes-manifests.mjs',
        '--image-digest',
        digest,
        '--website-tenant-fleet-name',
        tenantFleetName,
        '--website-node-name',
        nodeName,
        '--website-node-secret-name',
        secretName,
        '--website-trusted-proxy-cidr',
        '10.42.0.0/16',
      ],
      { cwd: REPO_ROOT, encoding: 'utf8', windowsHide: true },
    );
    assert.equal(rendered.status, 0, rendered.stderr);

    const deploymentText = readFileSync(path.join(outputDirectory, 'deployment.yaml'), 'utf8');
    assert.doesNotMatch(deploymentText, /__SDKWORK_/u);
    assert.match(deploymentText, new RegExp(`sha256:${digest}`, 'u'));
    const deploymentDocuments = parseAllDocuments(deploymentText).map((document) =>
      document.toJSON(),
    );
    const statefulSet = deploymentDocuments.find(
      (document) => document?.kind === 'StatefulSet',
    );
    assert.ok(statefulSet);
    assert.equal(
      statefulSet.metadata.name,
      `sdkwork-web-node-${tenantFleetName}-${nodeName}`,
    );
    assert.equal(
      statefulSet.metadata.labels['sdkwork.com/tenant-fleet'],
      tenantFleetName,
    );
    assert.equal(
      statefulSet.spec.selector.matchLabels['sdkwork.com/tenant-fleet'],
      tenantFleetName,
    );
    assert.equal(
      statefulSet.spec.template.metadata.labels['sdkwork.com/tenant-fleet'],
      tenantFleetName,
    );
    assert.equal(
      statefulSet.spec.serviceName,
      `sdkwork-web-website-${tenantFleetName}-headless`,
    );
    assert.equal(statefulSet.spec.replicas, 1);
    assert.equal(statefulSet.spec.template.spec.enableServiceLinks, false);
    assert.deepEqual(
      statefulSet.spec.template.spec.topologySpreadConstraints.map((constraint) => ({
        topologyKey: constraint.topologyKey,
        whenUnsatisfiable: constraint.whenUnsatisfiable,
        tenantFleet: constraint.labelSelector.matchLabels['sdkwork.com/tenant-fleet'],
      })),
      [
        {
          topologyKey: 'kubernetes.io/hostname',
          whenUnsatisfiable: 'DoNotSchedule',
          tenantFleet: tenantFleetName,
        },
        {
          topologyKey: 'topology.kubernetes.io/zone',
          whenUnsatisfiable: 'ScheduleAnyway',
          tenantFleet: tenantFleetName,
        },
      ],
    );
    const container = statefulSet.spec.template.spec.containers[0];
    assert.deepEqual(container.command, [
      '/app/bin/sdkwork-webserver-website-delivery-edge-runtime',
    ]);
    assert.equal(container.ports[0].containerPort, 8080);
    assert.equal(container.resources.requests['ephemeral-storage'], '128Mi');
    assert.equal(container.resources.limits['ephemeral-storage'], '256Mi');
    assert.ok(
      container.env.some(
        (entry) =>
          entry.name === 'SDKWORK_WEBSERVER_NODE_UUID' &&
          entry.valueFrom?.secretKeyRef?.name === secretName,
      ),
    );
    assert.ok(
      container.env.some(
        (entry) =>
          entry.name === 'SDKWORK_WEBSERVER_NODE_TOKEN_FILE' &&
          entry.value === '/run/secrets/sdkwork-web-node/node-token',
      ),
    );
    assert.ok(
      container.env.some(
        (entry) =>
          entry.name === 'SDKWORK_WEBSERVER_SERVER_CONFIG_FILE' &&
          entry.value === '/etc/sdkwork/webserver/sdkwork.webserver.config.json',
      ),
    );
    assert.ok(container.volumeMounts.some((mount) => mount.name === 'host-config'));
    assert.equal(
      container.readinessProbe.exec.command[0],
      '/app/bin/sdkwork-webserver-website-delivery-edge-runtime',
    );
    assert.equal(container.readinessProbe.exec.command[2], '/readyz');
    assert.ok(container.volumeMounts.some((mount) => mount.name === 'recovery'));
    assert.ok(
      statefulSet.spec.volumeClaimTemplates.some(
        (claim) => claim.metadata.name === 'recovery',
      ),
    );
    assert.ok(
      statefulSet.spec.template.spec.volumes.some(
        (volume) => volume.secret?.secretName === secretName,
      ),
    );

    const serviceDocuments = parseAllDocuments(
      readFileSync(path.join(outputDirectory, 'service.yaml'), 'utf8'),
    ).map((document) => document.toJSON());
    assert.equal(serviceDocuments.filter((document) => document?.kind === 'Service').length, 3);
    assert.deepEqual(
      serviceDocuments.map((document) => document.metadata.name).sort(),
      [
        `sdkwork-web-events-${tenantFleetName}-${nodeName}`,
        `sdkwork-web-website-${tenantFleetName}`,
        `sdkwork-web-website-${tenantFleetName}-headless`,
      ],
    );
    assert.ok(
      serviceDocuments.every(
        (document) =>
          document.metadata.labels['sdkwork.com/tenant-fleet'] === tenantFleetName &&
          document.spec.selector['sdkwork.com/tenant-fleet'] === tenantFleetName,
      ),
    );
    assert.ok(
      serviceDocuments.some(
        (document) => document.spec.ports[0].targetPort === 'website-http',
      ),
    );
    assert.ok(
      serviceDocuments.some(
        (document) => document.spec.ports[0].targetPort === 'provider-events',
      ),
    );
    const providerEventService = serviceDocuments.find(
      (document) => document.spec.ports[0].targetPort === 'provider-events',
    );
    assert.equal(providerEventService.spec.selector['sdkwork.com/web-node'], nodeName);
    const relay = statefulSet.spec.template.spec.containers.find(
      (candidate) => candidate.name === 'provider-event-relay',
    );
    assert.ok(relay);
    assert.deepEqual(relay.command, [
      '/app/bin/sdkwork-webserver-website-delivery-edge-runtime',
      'relay-provider-events',
    ]);
    assert.equal(relay.ports[0].containerPort, 3811);
    assert.equal(relay.resources.requests['ephemeral-storage'], '16Mi');
    assert.equal(relay.resources.limits['ephemeral-storage'], '64Mi');

    const migrationJob = parseYaml(
      readFileSync(path.join(outputDirectory, 'migration-job.yaml'), 'utf8'),
    );
    assert.equal(migrationJob.spec.template.spec.enableServiceLinks, false);
    const migrationContainer = migrationJob.spec.template.spec.containers[0];
    assert.equal(migrationContainer.resources.requests['ephemeral-storage'], '64Mi');
    assert.equal(migrationContainer.resources.limits['ephemeral-storage'], '128Mi');

    const configMap = parseYaml(
      readFileSync(path.join(outputDirectory, 'config-map.yaml'), 'utf8'),
    );
    assert.equal(configMap.kind, 'ConfigMap');
    assert.equal(configMap.immutable, true);
    const hostConfig = JSON.parse(configMap.data['sdkwork.webserver.config.json']);
    const configRevision = createHash('sha256')
      .update(configMap.data['sdkwork.webserver.config.json'])
      .digest('hex');
    assert.equal(
      configMap.metadata.name,
      `sdkwork-web-website-config-${tenantFleetName}-${nodeName}-${configRevision.slice(0, 16)}`,
    );
    assert.equal(configMap.metadata.labels['sdkwork.com/tenant-fleet'], tenantFleetName);
    assert.equal(
      configMap.metadata.labels['sdkwork.com/config-revision'],
      configRevision.slice(0, 16),
    );
    assert.equal(configMap.metadata.annotations['sdkwork.com/config-sha256'], configRevision);
    assert.ok(
      statefulSet.spec.template.spec.volumes.some(
        (volume) => volume.configMap?.name === configMap.metadata.name,
      ),
    );
    assert.deepEqual(hostConfig.listeners[0].trustedProxy.trustedCidrs, ['10.42.0.0/16']);
    assert.equal(hostConfig.listeners[0].trustedProxy.header, 'x-forwarded-for');

    const networkPolicy = parseYaml(
      readFileSync(path.join(outputDirectory, 'network-policy.yaml'), 'utf8'),
    );
    assert.equal(networkPolicy.kind, 'NetworkPolicy');
    assert.equal(
      networkPolicy.spec.podSelector.matchLabels['sdkwork.com/tenant-fleet'],
      tenantFleetName,
    );
    assert.deepEqual(
      networkPolicy.spec.ingress.map((rule) => rule.ports[0].port),
      [8080, 3811],
    );
    assert.ok(
      networkPolicy.spec.ingress.every(
        (rule) =>
          rule.from[0].namespaceSelector?.matchLabels?.['sdkwork.com/network-role'] &&
          rule.from[0].podSelector?.matchLabels?.['sdkwork.com/network-role'],
      ),
    );
    const disruptionBudget = deploymentDocuments.find(
      (document) => document?.kind === 'PodDisruptionBudget',
    );
    assert.equal(
      disruptionBudget.spec.selector.matchLabels['sdkwork.com/tenant-fleet'],
      tenantFleetName,
    );
    assert.equal(disruptionBudget.spec.maxUnavailable, 1);
    assert.equal(disruptionBudget.spec.minAvailable, undefined);
  } finally {
    rmSync(outputDirectory, { recursive: true, force: true });
  }
});

test('Kubernetes renderer rejects universal website trusted-proxy networks', () => {
  const digest = 'cd'.repeat(32);
  const tenantFleetName = 'tf-klmno23456pqrst';
  const nodeName = `contract-unsafe-${process.pid}`;
  const outputDirectory = path.join(
    REPO_ROOT,
    '.sdkwork',
    'runtime',
    'kubernetes',
    `${digest.slice(0, 16)}-${tenantFleetName}-${nodeName}`,
  );
  rmSync(outputDirectory, { recursive: true, force: true });
  const rendered = spawnSync(
    process.execPath,
    [
      'scripts/render-kubernetes-manifests.mjs',
      '--image-digest',
      digest,
      '--website-tenant-fleet-name',
      tenantFleetName,
      '--website-node-name',
      nodeName,
      '--website-node-secret-name',
      `${nodeName}-secret`,
      '--website-trusted-proxy-cidr',
      '0.0.0.0/0',
    ],
    { cwd: REPO_ROOT, encoding: 'utf8', windowsHide: true },
  );
  assert.notEqual(rendered.status, 0);
  assert.match(rendered.stderr, /universal trusted proxy CIDRs are forbidden/u);
  assert.equal(existsSync(outputDirectory), false);
});

test('Kubernetes renderer requires an opaque non-identifying tenant fleet label', () => {
  const digest = 'ef'.repeat(32);
  const commonArgs = [
    'scripts/render-kubernetes-manifests.mjs',
    '--image-digest',
    digest,
    '--website-node-name',
    'contract-node',
    '--website-node-secret-name',
    'contract-node-secret',
    '--website-trusted-proxy-cidr',
    '10.42.0.0/16',
  ];
  const missing = spawnSync(process.execPath, commonArgs, {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    windowsHide: true,
  });
  assert.notEqual(missing.status, 0);
  assert.match(missing.stderr, /--website-tenant-fleet-name/u);

  const oversized = spawnSync(
    process.execPath,
    [...commonArgs, '--website-tenant-fleet-name', 'tenant-100001'],
    { cwd: REPO_ROOT, encoding: 'utf8', windowsHide: true },
  );
  assert.notEqual(oversized.status, 0);
  assert.match(oversized.stderr, /tf- followed by exactly 15 lowercase base32 characters/u);

  const duplicate = spawnSync(
    process.execPath,
    [
      ...commonArgs,
      '--website-tenant-fleet-name',
      'tf-aaaaa22222bbbbb',
      '--website-tenant-fleet-name',
      'tf-ccccc33333ddddd',
    ],
    { cwd: REPO_ROOT, encoding: 'utf8', windowsHide: true },
  );
  assert.notEqual(duplicate.status, 0);
  assert.match(duplicate.stderr, /cannot be provided more than once/u);
});

test('Kubernetes renderer keeps equal Node labels isolated across tenant fleets', () => {
  const digest = '12'.repeat(32);
  const nodeName = `shared-node-${process.pid}`;
  const renderedFleets = [];
  const outputDirectories = [];
  try {
    for (const tenantFleetName of ['tf-aaaaa22222bbbbb', 'tf-ccccc33333ddddd']) {
      const outputDirectory = path.join(
        REPO_ROOT,
        '.sdkwork',
        'runtime',
        'kubernetes',
        `${digest.slice(0, 16)}-${tenantFleetName}-${nodeName}`,
      );
      rmSync(outputDirectory, { recursive: true, force: true });
      outputDirectories.push(outputDirectory);
      const rendered = spawnSync(
        process.execPath,
        [
          'scripts/render-kubernetes-manifests.mjs',
          '--image-digest',
          digest,
          '--website-tenant-fleet-name',
          tenantFleetName,
          '--website-node-name',
          nodeName,
          '--website-node-secret-name',
          `${tenantFleetName}-secret`,
          '--website-trusted-proxy-cidr',
          '10.42.0.0/16',
        ],
        { cwd: REPO_ROOT, encoding: 'utf8', windowsHide: true },
      );
      assert.equal(rendered.status, 0, rendered.stderr);
      const services = parseAllDocuments(
        readFileSync(path.join(outputDirectory, 'service.yaml'), 'utf8'),
      ).map((document) => document.toJSON());
      const deployment = parseAllDocuments(
        readFileSync(path.join(outputDirectory, 'deployment.yaml'), 'utf8'),
      )
        .map((document) => document.toJSON())
        .find((document) => document?.kind === 'StatefulSet');
      renderedFleets.push({ tenantFleetName, outputDirectory, services, deployment });
    }

    const [alpha, beta] = renderedFleets;
    assert.notEqual(alpha.outputDirectory, beta.outputDirectory);
    assert.notEqual(alpha.deployment.metadata.name, beta.deployment.metadata.name);
    assert.ok(
      alpha.services.every(
        (service) =>
          service.spec.selector['sdkwork.com/tenant-fleet'] === alpha.tenantFleetName,
      ),
    );
    assert.ok(
      beta.services.every(
        (service) =>
          service.spec.selector['sdkwork.com/tenant-fleet'] === beta.tenantFleetName,
      ),
    );
    const alphaNames = new Set(alpha.services.map((service) => service.metadata.name));
    assert.ok(beta.services.every((service) => !alphaNames.has(service.metadata.name)));
  } finally {
    for (const outputDirectory of outputDirectories) {
      rmSync(outputDirectory, { recursive: true, force: true });
    }
  }
});

async function createFixture(options) {
  const profile = options.profile ?? 'standalone';
  const architecture = options.architecture ?? 'x64';
  const version = options.version;
  const artifactBase = `sdkwork-web-linux-${architecture}-${profile}-server-${version}`;
  const archive = path.join(OUTPUT_ROOT, `${artifactBase}.tar.gz`);
  const checksum = `${archive}.sha256`;
  const temporaryRoot = mkdtempSync(path.join(tmpdir(), 'sdkwork-web-release-fixture-'));
  const stageRoot = path.join(temporaryRoot, 'sdkwork-web');
  const content = [];
  const packageFiles = packageFilesFor(profile, options);
  const archiveDirectories = archiveDirectoriesFor(packageFiles);

  for (const [relativePath, text] of packageFiles) {
    const filePath = path.join(stageRoot, ...relativePath.split('/'));
    mkdirSync(path.dirname(filePath), { recursive: true });
    writeFileSync(filePath, text, 'utf8');
    chmodSync(
      filePath,
      relativePath.startsWith('bin/') ? (options.binaryMode ?? 0o755) : 0o644,
    );
    const bytes = readFileSync(filePath);
    content.push({ path: relativePath, bytes: bytes.length, sha256: sha256(bytes) });
  }
  content.sort((left, right) => (left.path < right.path ? -1 : left.path > right.path ? 1 : 0));
  const manifest = {
    schemaVersion: 1,
    kind: 'sdkwork.server-package',
    application: 'sdkwork-web',
    version,
    deploymentProfile: profile,
    runtimeTarget: 'server',
    platform: 'linux',
    architecture,
    sourceDateEpoch: 0,
    content,
  };
  options.mutateManifest?.(manifest);
  const manifestPath = path.join(stageRoot, 'package.manifest.json');
  writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
  chmodSync(manifestPath, 0o644);

  const additionalEntries = [];
  if (options.extraFile) {
    const extraPath = path.join(stageRoot, 'etc', 'unexpected.txt');
    writeFileSync(extraPath, 'unexpected\n', 'utf8');
    chmodSync(extraPath, 0o644);
    additionalEntries.push('sdkwork-web/etc/unexpected.txt');
  }
  if (options.symbolicLink) {
    const linkPath = path.join(stageRoot, 'etc', 'unsafe-link');
    symlinkSync('../sdkwork.app.config.json', linkPath, 'file');
    additionalEntries.push('sdkwork-web/etc/unsafe-link');
  }
  const entries = [
    ...archiveDirectories,
    'sdkwork-web/package.manifest.json',
    ...content.map((item) => `sdkwork-web/${item.path}`),
    ...additionalEntries,
  ].sort();
  if (options.duplicateEntry) {
    entries.push('sdkwork-web/sdkwork.app.config.json');
  }

  mkdirSync(OUTPUT_ROOT, { recursive: true });
  rmSync(archive, { force: true });
  rmSync(checksum, { force: true });
  await create(
    {
      file: archive,
      cwd: temporaryRoot,
      gzip: { portable: true },
      portable: false,
      mtime: new Date(0),
      noDirRecurse: true,
      filter(entryPath, stat) {
        stat.uid = 0;
        stat.gid = 0;
        stat.mode = entryPath.startsWith('sdkwork-web/bin/')
          ? 0o100000 | (options.binaryMode ?? 0o755)
          : stat.isDirectory()
            ? 0o040755
            : 0o100644;
        return true;
      },
    },
    entries,
  );
  const archiveBytes = readFileSync(archive);
  writeFileSync(checksum, `${sha256(archiveBytes)}  ${path.basename(archive)}\n`, 'utf8');
  rmSync(temporaryRoot, { recursive: true, force: true });
  return {
    archive,
    checksum,
    cleanup() {
      rmSync(archive, { force: true });
      rmSync(checksum, { force: true });
      rmSync(`${archive}.cdx.json`, { force: true });
      rmSync(`${archive}.cdx.json.sha256`, { force: true });
    },
  };
}

test('workspace and workflow close the frozen release dependency graph', () => {
  const packageJson = JSON.parse(readFileSync(path.join(REPO_ROOT, 'package.json'), 'utf8'));
  const workspace = parseYaml(readFileSync(path.join(REPO_ROOT, 'pnpm-workspace.yaml'), 'utf8'));
  const lockfile = parseYaml(readFileSync(path.join(REPO_ROOT, 'pnpm-lock.yaml'), 'utf8'));
  const workflow = JSON.parse(readFileSync(path.join(REPO_ROOT, 'sdkwork.workflow.json'), 'utf8'));
  const thinWorkflow = readFileSync(path.join(REPO_ROOT, '.github/workflows/package.yml'), 'utf8');

  assert.equal(packageJson.dependencies.tar, '7.5.21');
  assert.equal(packageJson.dependencies['@sdkwork/app-topology'], 'workspace:*');
  assert.ok(workspace.packages.includes('../sdkwork-app-topology'));
  assert.ok(workspace.packages.includes('../sdkwork-sdk-commons/sdkwork-sdk-common-typescript'));
  assert.equal(lockfile.importers['.'].dependencies.tar.specifier, '7.5.21');
  assert.equal(
    lockfile.importers['.'].dependencies['@sdkwork/app-topology'].version,
    'link:../sdkwork-app-topology',
  );
  const dependencyIds = new Set(workflow.dependencies.map((dependency) => dependency.id));
  for (const dependencyId of ['sdkwork-core', 'sdkwork-ui', 'sdkwork-sdk-commons']) {
    assert.ok(dependencyIds.has(dependencyId));
  }
  for (const ref of ['SDKWORK_CORE_REF', 'SDKWORK_UI_REF', 'SDKWORK_SDK_COMMONS_REF']) {
    assert.match(thinWorkflow, new RegExp(ref, 'u'));
  }
  assert.ok(
    workflow.lifecycle.validate.some(
      (step) => step.run === 'node scripts/webserver-release.mjs validate',
    ),
  );
  const archiveValidationIndex = workflow.lifecycle.validate.findIndex(
    (step) => step.run === 'node scripts/webserver-release.mjs validate',
  );
  const runtimeSmokeIndex = workflow.lifecycle.validate.findIndex(
    (step) => step.run === 'node scripts/webserver-release-smoke.mjs',
  );
  assert.ok(archiveValidationIndex >= 0 && runtimeSmokeIndex > archiveValidationIndex);
  assert.equal(workflow.security.sbomRequired, true);
  assert.equal(workflow.security.signingRequired, true);
  assert.ok(
    workflow.lifecycle.sign.some((step) => step.run === 'node scripts/webserver-sign.mjs sign'),
  );
  assert.ok(
    workflow.lifecycle.validate.some(
      (step) => step.run === 'node scripts/webserver-sign.mjs verify',
    ),
  );
  assert.ok(
    workflow.lifecycle.sbom.some((step) => step.run === 'node scripts/webserver-sbom.mjs generate'),
  );
  const sbomValidationIndex = workflow.lifecycle.validate.findIndex(
    (step) => step.run === 'node scripts/webserver-sbom.mjs validate',
  );
  assert.ok(sbomValidationIndex > runtimeSmokeIndex);
});

test('Linux release smoke validates, extracts, serves HTTP and HTTPS, and cleans up', () => {
  const packageJson = JSON.parse(readFileSync(path.join(REPO_ROOT, 'package.json'), 'utf8'));
  assert.equal(
    packageJson.scripts['release:smoke:standalone'],
    'node scripts/webserver-release-smoke.mjs --deployment-profile standalone',
  );
  assert.equal(
    packageJson.scripts['release:smoke:cloud'],
    'node scripts/webserver-release-smoke.mjs --deployment-profile cloud',
  );
  const source = readFileSync(
    path.join(REPO_ROOT, 'scripts/webserver-release-smoke.mjs'),
    'utf8',
  );
  assert.match(source, /SUPPORTED_ARCHITECTURES = new Set\(\['x64', 'arm64'\]\)/u);
  assert.match(source, /process\.platform !== 'linux' \|\| process\.arch !== resolved\.architecture/u);
  assert.match(source, /scripts\/webserver-release\.mjs/u);
  assert.match(source, /extractTar/u);
  assert.match(source, /preservePaths: false/u);
  assert.match(source, /openssl/u);
  assert.match(source, /sdkwork-webserver-website-delivery-edge-runtime/u);
  assert.match(source, /run\(websiteEdgeRuntime, \['--help'\]/u);
  assert.match(source, /run\(websiteEdgeRuntime, \['validate', packagedWebsiteHostConfig\]/u);
  assert.match(source, /share', 'sdkwork', 'webserver-pc'/u);
  assert.match(source, /run\(gateway, \['validate-app-shell'\]/u);
  assert.match(source, /SDKWORK_IAM_APP_ROOT/u);
  assert.match(source, /SDKWORK_DRIVE_APP_ROOT/u);
  assert.match(source, /SDKWORK_WEBSERVER_SNOWFLAKE_NODE_ID: '0'/u);
  assert.match(source, /SDKWORK_WEBSERVER_SECRET_ENCRYPTION_KEY/u);
  assert.match(source, /SDKWORK_WEBSERVER_ACME_PROFILE: 'staging'/u);
  assert.match(source, /SDKWORK_WEBSERVER_ACME_CONTACT_EMAIL/u);
  assert.match(source, /SDKWORK_DRIVE_DOWNLOAD_TOKEN_HMAC_SECRET/u);
  assert.match(source, /applications\.list/u);
  assert.match(source, /sessions\.current\.retrieve/u);
  assert.match(source, /assets\.list/u);
  assert.match(source, /application\/problem\+json/u);
  assert.match(source, /instance: `GET \$\{ownerRoute\.path\}`/u);
  assert.match(source, /operationId: ownerRoute\.operationId/u);
  assert.match(source, /\['data-plane', smokeConfigPath\]/u);
  assert.match(source, /waitForHealth\('http'/u);
  assert.match(source, /waitForHealth\('https'/u);
  assert.match(source, /child\.kill\('SIGTERM'\)/u);
  assert.match(source, /rmSync\(temporaryRoot, \{ recursive: true, force: true \}\)/u);
});

test('bounded release validator accepts an exact immutable archive', async () => {
  const version = '9.8.7-valid';
  const fixture = await createFixture({ version });
  try {
    const result = runValidator('standalone', version);
    assert.equal(result.status, 0, result.stderr);
    assert.match(
      result.stdout,
      new RegExp(
        `validated artifact=.* bytes=[0-9]+ entries=${expectedArchiveEntries('standalone')}`,
        'u',
      ),
    );
  } finally {
    fixture.cleanup();
  }
});

test('bounded release validator accepts cloud only when PC assets are absent', async () => {
  const validVersion = '9.8.7-cloud-valid';
  const valid = await createFixture({ profile: 'cloud', version: validVersion });
  try {
    const result = runValidator('cloud', validVersion);
    assert.equal(result.status, 0, result.stderr);
    assert.match(
      result.stdout,
      new RegExp(
        `validated artifact=.* bytes=[0-9]+ entries=${expectedArchiveEntries('cloud')}`,
        'u',
      ),
    );
  } finally {
    valid.cleanup();
  }

  const invalidVersion = '9.8.7-cloud-with-pc';
  const invalid = await createFixture({
    profile: 'cloud',
    version: invalidVersion,
    includePcAssets: true,
  });
  try {
    const result = runValidator('cloud', invalidVersion);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /cloud package must not contain PC standalone static assets/u);
  } finally {
    invalid.cleanup();
  }

  const invalidDependencyVersion = '9.8.7-cloud-with-dependencies';
  const invalidDependency = await createFixture({
    profile: 'cloud',
    version: invalidDependencyVersion,
    includeDependencyAssets: true,
  });
  try {
    const result = runValidator('cloud', invalidDependencyVersion);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /cloud package must not contain standalone dependency runtime assets/u);
  } finally {
    invalidDependency.cleanup();
  }
});

test('standalone release requires valid same-origin PC bootstrap files', async () => {
  for (const [name, options, expectedError] of [
    [
      'missing-index',
      { omitFiles: new Set(['share/sdkwork/webserver-pc/index.html']) },
      /must contain PC index\.html and runtime-env\.json/u,
    ],
    [
      'missing-runtime-env',
      { omitFiles: new Set(['share/sdkwork/webserver-pc/runtime-env.json']) },
      /must contain PC index\.html and runtime-env\.json/u,
    ],
    [
      'missing-web-iam-module',
      { omitFiles: new Set(['specs/iam.module.manifest.json']) },
      /package manifest content paths are missing, unexpected, duplicated, or unsorted/u,
    ],
    [
      'cross-origin-runtime-env',
      {
        fileOverrides: new Map([
          [
            'share/sdkwork/webserver-pc/runtime-env.json',
            STANDALONE_PC_FILES.get('share/sdkwork/webserver-pc/runtime-env.json').replace(
              '"appApiBaseUrl": "/"',
              '"appApiBaseUrl": "http://127.0.0.1:3900"',
            ),
          ],
        ]),
      },
      /runtime-env\.json\.appApiBaseUrl must use the canonical same-origin root/u,
    ],
    [
      'missing-iam-runtime',
      { omitFiles: new Set(['share/sdkwork/iam/database/database.manifest.json']) },
      /iam runtime assets require non-empty database\/database\.manifest\.json/u,
    ],
    [
      'missing-drive-runtime',
      { omitFiles: new Set(['share/sdkwork/drive/database/database.manifest.json']) },
      /drive runtime assets require non-empty database\/database\.manifest\.json/u,
    ],
  ]) {
    const version = `9.8.7-${name}`;
    const fixture = await createFixture({ version, ...options });
    try {
      const result = runValidator('standalone', version);
      assert.notEqual(result.status, 0, name);
      assert.match(result.stderr, expectedError, name);
    } finally {
      fixture.cleanup();
    }
  }
});

test('CycloneDX SBOM binds the archive and locked Cargo closure and rejects semantic tampering', async () => {
  const version = '9.8.7-sbom';
  const fixture = await createFixture({ version });
  const sbomPath = `${fixture.archive}.cdx.json`;
  const checksumPath = `${sbomPath}.sha256`;
  try {
    const missing = runSbom('validate', 'standalone', version);
    assert.notEqual(missing.status, 0);
    assert.match(missing.stderr, /release SBOM does not exist/u);

    const generated = runSbom('generate', 'standalone', version);
    assert.equal(generated.status, 0, generated.stderr);
    const sbom = JSON.parse(readFileSync(sbomPath, 'utf8'));
    assert.equal(sbom.bomFormat, 'CycloneDX');
    assert.equal(sbom.specVersion, '1.6');
    assert.equal(sbom.metadata.component.version, version);
    assert.equal(sbom.metadata.component.hashes[0].content, sha256(readFileSync(fixture.archive)));
    assert.ok(sbom.components.length > 0 && sbom.components.length <= 20_000);
    for (const packageName of [
      'sdkwork-web-agent',
      'sdkwork-api-webserver-standalone-gateway',
      'sdkwork-webserver-website-delivery-edge-runtime',
      'sdkwork-webserver-certificate-worker',
    ]) {
      assert.ok(sbom.components.some((component) => component.name === packageName));
    }
    const valid = runSbom('validate', 'standalone', version);
    assert.equal(valid.status, 0, valid.stderr);

    sbom.metadata.component.name = 'sdkwork-web-tampered';
    const tamperedText = `${JSON.stringify(sbom, null, 2)}\n`;
    writeFileSync(sbomPath, tamperedText, 'utf8');
    writeFileSync(checksumPath, `${sha256(tamperedText)}  ${path.basename(sbomPath)}\n`, 'utf8');
    const tampered = runSbom('validate', 'standalone', version);
    assert.notEqual(tampered.status, 0);
    assert.match(
      tampered.stderr,
      /release SBOM does not match the artifact and locked Cargo dependency closure/u,
    );
  } finally {
    fixture.cleanup();
  }
});

test('bounded release validator binds an arm64 archive to its manifest architecture', async () => {
  const version = '9.8.7-arm64';
  const fixture = await createFixture({ version, architecture: 'arm64' });
  try {
    const result = runValidator('standalone', version, 'arm64');
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /validated artifact=sdkwork-web-linux-arm64-/u);

    const wrongArchitecture = runValidator('standalone', version, 'x64');
    assert.notEqual(wrongArchitecture.status, 0);
    assert.match(
      wrongArchitecture.stderr,
      /sdkwork-web-linux-x64-standalone-server-9\.8\.7-arm64\.tar\.gz/u,
    );
  } finally {
    fixture.cleanup();
  }
});

test('bounded release validator rejects a relabelled arm64 manifest', async () => {
  const version = '9.8.7-arm64-relabelled';
  const fixture = await createFixture({
    version,
    architecture: 'arm64',
    mutateManifest(manifest) {
      manifest.architecture = 'x64';
    },
  });
  try {
    const result = runValidator('standalone', version, 'arm64');
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /package manifest identity does not match the selected artifact/u);
  } finally {
    fixture.cleanup();
  }
});

test('release validator rejects a checksum sidecar mismatch before archive trust', async () => {
  const version = '9.8.7-checksum';
  const fixture = await createFixture({ version });
  try {
    writeFileSync(fixture.checksum, `${'0'.repeat(64)}  ${path.basename(fixture.archive)}\n`);
    const result = runValidator('standalone', version);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /SHA-256 does not match its sidecar/u);
  } finally {
    fixture.cleanup();
  }
});

test('release validator rejects manifest hashes that do not match streamed content', async () => {
  const version = '9.8.7-manifest';
  const fixture = await createFixture({
    version,
    mutateManifest(manifest) {
      manifest.content[0].sha256 = '0'.repeat(64);
    },
  });
  try {
    const result = runValidator('standalone', version);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /package content does not match manifest/u);
  } finally {
    fixture.cleanup();
  }
});

test('release validator rejects duplicate and unexpected archive entries', async () => {
  for (const [suffix, options, message] of [
    ['duplicate', { duplicateEntry: true }, /duplicate entry/u],
    ['extra', { extraFile: true }, /archive file inventory/u],
  ]) {
    const version = `9.8.7-${suffix}`;
    const fixture = await createFixture({ version, ...options });
    try {
      const result = runValidator('standalone', version);
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, message);
    } finally {
      fixture.cleanup();
    }
  }
});

test('release validator requires executable Linux server binaries', async () => {
  const version = '9.8.7-mode';
  const fixture = await createFixture({ version, binaryMode: 0o644 });
  try {
    const result = runValidator('standalone', version);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /archive binary .* must be executable/u);
  } finally {
    fixture.cleanup();
  }
});

test(
  'release validator rejects symbolic-link archive entries',
  { skip: process.platform === 'win32' ? 'tar cannot create a portable NTFS symlink fixture' : false },
  async () => {
    const version = '9.8.7-symlink';
    const fixture = await createFixture({ version, symbolicLink: true });
    try {
      const result = runValidator('standalone', version);
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /unsupported metadata or links|unsupported type SymbolicLink/u);
    } finally {
      fixture.cleanup();
    }
  },
);
