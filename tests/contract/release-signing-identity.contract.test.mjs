import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const SIGN_SCRIPT = path.join(REPO_ROOT, 'scripts', 'webserver-sign.mjs');

// The identity alternation lives in the script as JS string literals. It has to
// be decoded the way the module itself decodes it: building a RegExp straight
// from the raw source text matches a literal backslash instead of a dot, which
// would reject every real identity and make this test fail for the wrong reason.
function decodeJsStringLiteral(literal) {
  let out = '';
  for (let index = 0; index < literal.length; index += 1) {
    const character = literal[index];
    if (character !== '\\') {
      out += character;
      continue;
    }
    index += 1;
    const escaped = literal[index];
    if (escaped === 'n') out += '\n';
    else if (escaped === 'r') out += '\r';
    else if (escaped === 't') out += '\t';
    else out += escaped;
  }
  return out;
}

function readIdentityPattern() {
  const source = readFileSync(SIGN_SCRIPT, 'utf8');
  const literal = source.match(
    /const OFFICIAL_WORKFLOW_IDENTITIES = \[([\s\S]*?)\]\.join\('\|'\);/u,
  );
  assert.ok(literal, 'the sign script must declare OFFICIAL_WORKFLOW_IDENTITIES as an alternation');
  const branches = literal[1]
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter((line) => line.startsWith("'"))
    .map((line) => decodeJsStringLiteral(line.replace(/^'/u, '').replace(/',?$/u, '')));
  assert.equal(branches.length, 2, 'exactly two first-party identities are expected');
  return new RegExp(branches.join('|'));
}

test('signing accepts the own workflow and the pinned reusable workflow identities', () => {
  const pattern = readIdentityPattern();
  for (const identity of [
    'https://github.com/sdkwork-ai/sdkwork-webserver/.github/workflows/package.yml@refs/heads/main',
    'https://github.com/sdkwork-ai/sdkwork-webserver/.github/workflows/package.yml@refs/tags/v0.1.9',
    'https://github.com/sdkwork-ai/sdkwork-webserver/.github/workflows/package.yml@refs/pull/42/merge',
    'https://github.com/sdkwork-ai/sdkwork-github-workflow/.github/workflows/sdkwork-package.yml@bfe420b79d3c584d246f9691c8374b24534c3365',
  ]) {
    assert.ok(pattern.test(identity), `legal release identity was rejected: ${identity}`);
  }
});

test('signing still rejects every identity outside the pinned organization and refs', () => {
  const pattern = readIdentityPattern();
  for (const [label, identity] of [
    ['different owner', 'https://github.com/attacker/sdkwork-github-workflow/.github/workflows/sdkwork-package.yml@bfe420b79d3c584d246f9691c8374b24534c3365'],
    ['same org wrong repo', 'https://github.com/sdkwork-ai/other-repo/.github/workflows/sdkwork-package.yml@bfe420b79d3c584d246f9691c8374b24534c3365'],
    ['reusable workflow by mutable ref', 'https://github.com/sdkwork-ai/sdkwork-github-workflow/.github/workflows/sdkwork-package.yml@refs/heads/main'],
    ['reusable workflow by short sha', 'https://github.com/sdkwork-ai/sdkwork-github-workflow/.github/workflows/sdkwork-package.yml@bfe420b7'],
    ['own workflow by sha', 'https://github.com/sdkwork-ai/sdkwork-webserver/.github/workflows/package.yml@bfe420b79d3c584d246f9691c8374b24534c3365'],
    ['different workflow file', 'https://github.com/sdkwork-ai/sdkwork-github-workflow/.github/workflows/other.yml@bfe420b79d3c584d246f9691c8374b24534c3365'],
    ['non-github host', 'https://gitlab.com/sdkwork-ai/sdkwork-github-workflow/.github/workflows/sdkwork-package.yml@bfe420b79d3c584d246f9691c8374b24534c3365'],
    ['trailing path injection', 'https://github.com/sdkwork-ai/sdkwork-github-workflow/.github/workflows/sdkwork-package.yml@bfe420b79d3c584d246f9691c8374b24534c3365/../evil'],
    ['sha with trailing junk', 'https://github.com/sdkwork-ai/sdkwork-github-workflow/.github/workflows/sdkwork-package.yml@bfe420b79d3c584d246f9691c8374b24534c3365evil'],
    ['uppercase sha', 'https://github.com/sdkwork-ai/sdkwork-github-workflow/.github/workflows/sdkwork-package.yml@BFE420B79D3C584D246F9691C8374B24534C3365'],
    ['empty identity', ''],
  ]) {
    assert.equal(pattern.test(identity), false, `illegal identity was accepted [${label}]: ${identity}`);
  }
});

test('installer packaging steps are guarded against a foreign runner architecture', () => {
  const config = JSON.parse(readFileSync(path.join(REPO_ROOT, 'agent.workflow.json'), 'utf8'));
  const guarded = config.lifecycle.package.filter((step) =>
    /SDKWORK_PACKAGE_ARCHITECTURE/u.test(step.run),
  );
  // Only the deb/rpm steps may be skipped off x64. The immutable archive step
  // must stay unconditional or arm64 targets would produce no tarball at all.
  assert.equal(guarded.length, 2, 'exactly the two installer steps must be architecture guarded');
  for (const step of guarded) {
    assert.match(step.run, /-ne 'x64'/u, `${step.name} must skip when the runner is not x64`);
    assert.match(step.run, /exit 0/u, `${step.name} must exit successfully when skipped`);
  }
  const archiveStep = config.lifecycle.package.find((step) =>
    /webserver-release\.mjs package/u.test(step.run),
  );
  assert.ok(archiveStep, 'the immutable archive packaging step must exist');
  assert.equal(
    /SDKWORK_PACKAGE_ARCHITECTURE/u.test(archiveStep.run),
    false,
    'the immutable archive step must run on every target architecture',
  );
});
