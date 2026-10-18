import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const WORKFLOW = path.join(REPO_ROOT, 'sdkwork.workflow.json');

// Six platform jobs each check out the sibling repositories next to this one and
// run the same filtered build. The dependency chain writes into shared `dist/`
// directories, and pnpm's recursive filter defaults to parallel execution, so
// two packages inside one job can race on the same directory:

//   Error: ENOTEMPTY: directory not empty, rmdir
//     '../sdkwork-sdk-commons/sdkwork-sdk-common-typescript/dist/<subdir>'
//
// The directory being emptied belongs to a sibling repository, so this
// repository cannot fix it at the source; serialising the build removes the
// concurrency that provokes it.

function buildSteps() {
  const workflow = JSON.parse(readFileSync(WORKFLOW, 'utf8'));
  return (workflow.lifecycle?.build ?? []).filter((step) =>
    typeof step.run === 'string' && step.run.includes('--filter'),
  );
}

test('every filtered workspace build runs with concurrency pinned to one', () => {
  const steps = buildSteps();
  assert.ok(steps.length > 0, 'the workflow must still build the JS dependency chain');

  const filterInvocations = steps.flatMap((step) => step.run.match(/pnpm[^\n]*?--filter\b/gu) ?? []);
  assert.ok(
    filterInvocations.length >= 2,
    'both the PC and the H5 dependency chains are expected',
  );

  for (const invocation of filterInvocations) {
    assert.match(
      invocation,
      /--workspace-concurrency=1\b/u,
      `a filtered recursive build would run its dependency chain in parallel: ${invocation}`,
    );
  }
});

test('serialising the build does not change which packages are selected', () => {
  const steps = buildSteps();
  const runs = steps.map((step) => step.run);

  // The concurrency flag must sit alongside the existing selections, never
  // replace them: dropping a --filter would silently ship an unbuilt bundle.
  assert.ok(
    runs.some((run) => run.includes('@sdkwork/webserver-pc^...')),
    'the PC dependency chain must still be selected',
  );
  assert.ok(
    runs.some((run) => run.includes('@sdkwork/webserver-h5^...')),
    'the H5 dependency chain must still be selected',
  );
  for (const run of runs) {
    assert.match(run, /--if-present\b/u, 'the tolerant build flag must be preserved');
    assert.match(
      run,
      /--workspace-concurrency=1 --filter/u,
      'the concurrency flag belongs directly before the filter it governs',
    );
  }
});
