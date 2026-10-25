import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const WORKFLOW_CONFIG = path.join(REPO_ROOT, 'agent.workflow.json');

function readWorkflowConfig() {
  return JSON.parse(readFileSync(WORKFLOW_CONFIG, 'utf8'));
}

/// The shell body of the preflight step that puts cargo on PATH.
function readRustPreflightBody() {
  const config = readWorkflowConfig();
  const steps = config.lifecycle?.preflight ?? [];
  const step = steps.find((entry) => /rustup\.rs|rustup component/.test(entry.run ?? ''));
  assert.ok(step, 'a preflight step must set up the rust toolchain');
  return step.run;
}

test('the rust preflight installs a profile that carries rustfmt and clippy', () => {
  const body = readRustPreflightBody();

  // `--profile minimal` ships neither rustfmt nor clippy, and the verify
  // lifecycle opens with `cargo fmt -- --check` and runs clippy through the test
  // suite. Run 39068478572 failed all six package jobs on
  // `cargo-fmt is not installed for the toolchain '1.98.0'`.
  assert.doesNotMatch(
    body,
    /--profile minimal/u,
    'a minimal rustup profile omits rustfmt, which verify needs',
  );

  // The reusable workflow installs the same toolchain via
  // dtolnay/rust-toolchain@stable with no `components`, so a version-guarded
  // install never fires on the runner. The component install must therefore sit
  // outside the `if ! command -v cargo` block.
  const guardedInstall = body.indexOf('rustup.rs');
  const componentInstall = body.indexOf('rustup component add');
  assert.ok(componentInstall !== -1, 'the preflight must install rustfmt and clippy explicitly');
  assert.ok(
    guardedInstall !== -1 && componentInstall > guardedInstall,
    'the component install must run after the toolchain bootstrap, not inside its guard',
  );
  assert.match(componentInstall && body.slice(componentInstall), /rustfmt/u);
  assert.match(componentInstall && body.slice(componentInstall), /clippy/u);

  // `set -x` alone hides the failing command in a long log; the step must fail
  // loudly, and it must prove the components are really there.
  assert.match(body, /set -euxo pipefail/u);
  assert.match(body, /cargo fmt --version/u);
  assert.match(body, /cargo clippy --version/u);
});

test('the verify lifecycle actually runs the checks the preflight provisions for', () => {
  const config = readWorkflowConfig();
  const packageJson = JSON.parse(readFileSync(path.join(REPO_ROOT, 'package.json'), 'utf8'));
  const scripts = packageJson.scripts ?? {};

  // The lifecycle has no `verify` phase: `cargo fmt` is reached through the
  // validate phase's "Validate web standards" step (`pnpm verify`) and the
  // package scripts it delegates to. Walk the whole reference graph rather than
  // one level.
  const validateSteps = (config.lifecycle?.validate ?? []).map((step) => step.run ?? '');
  assert.ok(
    validateSteps.some((run) => /\bverify\b/u.test(run)),
    `the validate phase must run the web standards gate; steps: ${validateSteps.join(' | ')}`,
  );
  const referenced = new Set();
  const queue = validateSteps
    .flatMap((run) => [...run.matchAll(/\bpnpm (?:run )?([\w:-]+)/gu)].map((match) => match[1]));
  // `pnpm verify` -> `pnpm exec sdkwork-app verify` is the last hop this test can
  // see: the framework command resolves `_sdkwork:verify` internally. That hop
  // is the framework's contract, so seed the walk with the script it must run.
  queue.push('_sdkwork:verify');
  while (queue.length > 0) {
    const name = queue.pop();
    if (referenced.has(name)) {
      continue;
    }
    referenced.add(name);
    for (const nested of String(scripts[name] ?? '').matchAll(/pnpm (?:run )?([\w:-]+)/gu)) {
      queue.push(nested[1]);
    }
  }
  const verifyBodies = [...referenced].map((name) => `${name}\n${scripts[name] ?? ''}`).join('\n');

  // Guards the guard: if verify ever stops calling cargo fmt, this preflight
  // requirement becomes cargo cult and should be revisited.
  assert.ok(
    verifyBodies.includes('cargo fmt'),
    `nothing reachable from the validate phase runs \`cargo fmt\`; reached: ${[...referenced].join(', ')}`,
  );
  assert.ok(
    verifyBodies.includes('cargo test'),
    'the verify lifecycle must still run the Rust test suite',
  );
});
