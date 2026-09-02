import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function readJson(relativePath) {
  return JSON.parse(readFileSync(path.join(REPO_ROOT, relativePath), 'utf8'));
}

function runNode(args, env = {}) {
  const inherited = { ...process.env };
  delete inherited.SDKWORK_WEBSERVER_NODE_TOKEN;
  delete inherited.SDKWORK_WEBSERVER_AGENT_TOKEN;
  delete inherited.SDKWORK_PACKAGE_VERSION;
  delete inherited.SDKWORK_RELEASE_VERSION;
  delete inherited.SDKWORK_PACKAGE_ARCHITECTURE;
  return spawnSync(process.execPath, args, {
    cwd: REPO_ROOT,
    encoding: 'utf8',
    env: { ...inherited, ...env },
    windowsHide: true,
  });
}

test('root development commands select the explicit standalone development profile', () => {
  const packageJson = readJson('package.json');
  assert.equal(packageJson.scripts.dev, 'pnpm dev:standalone');
  assert.equal(
    packageJson.scripts['dev:standalone'],
    'pnpm exec sdkwork-app dev --deployment-profile standalone',
  );
  // Verify must exercise a dev dry-run of the app lifecycle.
  assert.match(packageJson.scripts['_sdkwork:verify'], /sdkwork-app dev .*--dry-run/u);

  const index = readJson('etc/sdkwork.deployment.config.json');
  assert.equal(index.defaultProfile, 'standalone.development');
  // Standalone-only refactor: the deployment index owns exactly the
  // standalone.{development,test,staging,production,demo} profiles.
  assert.deepEqual(Object.keys(index.profiles).sort(), [
    'standalone.demo',
    'standalone.development',
    'standalone.production',
    'standalone.staging',
    'standalone.test',
  ]);
  for (const [profileId, profile] of Object.entries(index.profiles)) {
    assert.equal(profile.config, `topology/${profileId}.env`);
  }
});

// Removed: "cloud development uses remote HTTPS surfaces and starts only local
// clients" — the cloud development profile and etc/topology/cloud.development.env
// were deleted by the standalone-only refactor.

test('release dry-runs produce architecture and workflow-version-bound artifact names', () => {
  const packageJson = readJson('package.json');
  assert.equal(
    packageJson.scripts['release:package:standalone'],
    'pnpm exec sdkwork-app release:package --deployment-profile standalone',
  );

  // Standalone-only refactor: one deployment profile, two release architectures.
  for (const architecture of ['x64', 'arm64']) {
    const result = runNode(
      ['scripts/webserver-release.mjs', 'package', '--deployment-profile', 'standalone', '--dry-run'],
      { SDKWORK_PACKAGE_VERSION: '9.8.7', SDKWORK_PACKAGE_ARCHITECTURE: architecture },
    );
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /deploymentProfile=standalone/u);
    assert.match(result.stdout, new RegExp(`architecture=${architecture}`, 'u'));
    assert.match(
      result.stdout,
      new RegExp(`artifact=sdkwork-webserver-linux-${architecture}-standalone-server-9\\.8\\.7\\.tar\\.gz`, 'u'),
    );
  }

  const conflict = runNode(
    ['scripts/webserver-release.mjs', 'package', '--deployment-profile', 'standalone', '--dry-run'],
    { SDKWORK_PACKAGE_VERSION: '9.8.7', SDKWORK_RELEASE_VERSION: '9.8.6' },
  );
  assert.notEqual(conflict.status, 0);
  assert.match(conflict.stderr, /SDKWORK_PACKAGE_VERSION conflicts with SDKWORK_RELEASE_VERSION/u);

  const unsupported = runNode(
    ['scripts/webserver-release.mjs', 'package', '--deployment-profile', 'standalone', '--dry-run'],
    { SDKWORK_PACKAGE_VERSION: '9.8.7', SDKWORK_PACKAGE_ARCHITECTURE: 'riscv64' },
  );
  assert.notEqual(unsupported.status, 0);
  assert.match(unsupported.stderr, /release architecture must be x64 or arm64/u);
});

test('actual Linux archive generation fails before build on a mismatched host', () => {
  const architecture = process.platform === 'linux' && process.arch === 'x64' ? 'arm64' : 'x64';
  const result = runNode(
    ['scripts/webserver-release.mjs', 'package', '--deployment-profile', 'standalone'],
    { SDKWORK_PACKAGE_VERSION: '9.8.7', SDKWORK_PACKAGE_ARCHITECTURE: architecture },
  );
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    new RegExp(`linux-${architecture} server archives must be packaged on a linux-${architecture} runner`, 'u'),
  );
});

test('release smoke fails before archive access on a mismatched host architecture', () => {
  const architecture = process.platform === 'linux' && process.arch === 'x64' ? 'arm64' : 'x64';
  const result = runNode(
    [
      'scripts/webserver-release-smoke.mjs',
      '--deployment-profile',
      'standalone',
      '--architecture',
      architecture,
      '--version',
      '9.8.7',
    ],
  );
  assert.notEqual(result.status, 0);
  assert.match(
    result.stderr,
    new RegExp(`Linux ${architecture} release smoke must run on a linux-${architecture} host`, 'u'),
  );
});

test('release workflow and archive implementation preserve immutable bounded package contracts', () => {
  const workflow = readJson('sdkwork.workflow.json');
  // The build pipeline owns additional generator steps; assert that the
  // release build step is present instead of pinning its position.
  assert.ok(
    workflow.lifecycle.build.some((step) => step.run.includes('node scripts/build.mjs --release')),
  );
  assert.ok(
    workflow.lifecycle.package.some((step) => step.run === 'node scripts/webserver-release.mjs package'),
  );
  // Standalone-only refactor: every release target packages the standalone
  // server archive; deb/rpm installer targets cover the test/production envs.
  assert.ok(workflow.targets.length > 0);
  for (const target of workflow.targets) {
    assert.equal(target.deploymentProfile, 'standalone');
    assert.equal(target.platform, 'linux');
    assert.ok(['x64', 'arm64'].includes(target.architecture));
    assert.equal(target.runner, target.architecture === 'arm64' ? 'ubuntu-24.04-arm' : 'ubuntu-24.04');
  }
  assert.deepEqual(
    [...new Set(workflow.targets.map((target) => target.architecture))].sort(),
    ['arm64', 'x64'],
  );
  for (const target of workflow.targets.filter((entry) => entry.formats.includes('tar.gz'))) {
    assert.deepEqual(target.formats, ['tar.gz']);
    assert.deepEqual(target.outputGlobs, [
      `dist/release/sdkwork-webserver-linux-${target.architecture}-standalone-server-*.tar.gz`,
      `dist/release/sdkwork-webserver-linux-${target.architecture}-standalone-server-*.tar.gz.sha256`,
      `dist/release/sdkwork-webserver-linux-${target.architecture}-standalone-server-*.tar.gz.sigstore.json`,
      `dist/release/sdkwork-webserver-linux-${target.architecture}-standalone-server-*.tar.gz.cdx.json`,
      `dist/release/sdkwork-webserver-linux-${target.architecture}-standalone-server-*.tar.gz.cdx.json.sha256`,
    ]);
  }
  assert.equal(workflow.security.sbomRequired, true);
  assert.ok(
    workflow.lifecycle.sbom.some((step) => step.run === 'node scripts/webserver-sbom.mjs generate'),
  );
  assert.ok(
    workflow.lifecycle.validate.some((step) => step.run === 'node scripts/webserver-sbom.mjs validate'),
  );

  const source = readFileSync(path.join(REPO_ROOT, 'scripts/webserver-release.mjs'), 'utf8');
  assert.match(source, /MAX_ARCHIVE_BYTES = 512 \* 1024 \* 1024/u);
  assert.match(source, /function resolveCargoTargetRoot\(\)/u);
  assert.match(source, /CARGO_TARGET_DIR/u);
  assert.match(source, /path\.join\(cargoTargetRoot, 'release', binary\)/u);
  assert.match(source, /SDKWORK_PACKAGE_VERSION/u);
  assert.match(source, /SOURCE_DATE_EPOCH/u);
  for (const argument of ["'--sort=name'", "'--owner=0'", "'--group=0'", "'--numeric-owner'"]) {
    assert.match(source, new RegExp(argument, 'u'));
  }
  assert.match(source, /package\.manifest\.json/u);
  assert.match(source, /sha256File\(archive\)/u);
  assert.match(source, /renameSync\(temporaryArchive, archive\)/u);
  assert.match(source, /SUPPORTED_ARCHITECTURES = new Set\(\['x64', 'arm64'\]\)/u);
  assert.match(source, /process\.platform !== 'linux' \|\| process\.arch !== architecture/u);
  assert.ok(source.indexOf("process.platform !== 'linux'") < source.indexOf('ensureCriticalSources();'));
  assert.match(source, /source: 'etc\/examples\/public\/index\.html'/u);
  assert.match(source, /target: 'etc\/node-daemon\/development\.env\.example'/u);
  assert.match(source, /PC_PACKAGE_PREFIX = 'share\/sdkwork\/webserver-pc'/u);
  assert.match(source, /deploymentProfile === 'standalone'/u);
  // The browser bundles are built through the shared adaptive-web runner.
  assert.match(source, /build-browser-client\.mjs/u);
  assert.match(source, /inspectPcBuildOutput\(/u);
  assert.match(source, /inspectH5BuildOutput\(/u);
  assert.match(source, /inspectDependencyRuntimeAssets\(\)/u);
  assert.match(source, /share\/sdkwork\/iam/u);
  assert.match(source, /share\/sdkwork\/drive/u);
});
