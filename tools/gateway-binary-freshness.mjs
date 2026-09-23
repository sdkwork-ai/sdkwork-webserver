/**
 * Guard against measuring a gateway binary older than the sources it claims to
 * represent.
 *
 * Both nginx runners pick the newest *existing* build, which is exactly wrong
 * when the newest build is also stale: the workspace `target/debug` tree cannot
 * be refreshed while an operator's server is running out of it, so a runner
 * silently exercises the previous revision and reports a green result for code
 * that never ran. That happened: a differential run failed seven cases against
 * `target/debug` while the freshly built copy in `CARGO_TARGET_DIR` passed all
 * fifty.
 *
 * Comparing the binary's mtime against the newest Rust source turns that silent
 * false result into an explicit instruction.
 */
import fs from 'node:fs';
import path from 'node:path';

const SOURCE_DIRECTORIES = ['crates', 'src'];
const SOURCE_FILES = ['Cargo.toml', 'Cargo.lock'];
const SKIPPED_DIRECTORIES = new Set(['target', 'node_modules', '.git']);
/** Clock granularity / checkout skew allowance. */
const TOLERANCE_MS = 1000;

function visit(target, onFile) {
  let stat;
  try {
    stat = fs.statSync(target);
  } catch {
    return;
  }
  if (stat.isDirectory()) {
    let entries;
    try {
      entries = fs.readdirSync(target, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (SKIPPED_DIRECTORIES.has(entry.name)) continue;
      visit(path.join(target, entry.name), onFile);
    }
    return;
  }
  onFile(target, stat);
}

/** Newest mtime across the Rust sources that can change the gateway's behaviour. */
export function newestSourceMtime(root) {
  let newest = 0;
  const onFile = (file, stat) => {
    if (!file.endsWith('.rs') && !SOURCE_FILES.some((name) => file.endsWith(name))) return;
    if (stat.mtimeMs > newest) newest = stat.mtimeMs;
  };
  for (const rel of SOURCE_DIRECTORIES) visit(path.join(root, rel), onFile);
  for (const rel of SOURCE_FILES) visit(path.join(root, rel), onFile);
  return newest;
}

/**
 * @returns {{stale: boolean, binaryMtime: number, sourceMtime: number}}
 */
export function staleness(binary, root) {
  let binaryMtime = 0;
  try {
    binaryMtime = fs.statSync(binary).mtimeMs;
  } catch {
    return { stale: false, binaryMtime: 0, sourceMtime: 0 };
  }
  const sourceMtime = newestSourceMtime(root);
  if (sourceMtime === 0) return { stale: false, binaryMtime, sourceMtime };
  return {
    stale: binaryMtime + TOLERANCE_MS < sourceMtime,
    binaryMtime,
    sourceMtime,
  };
}

/** An actionable message: which binary is stale, and the two ways to fix it. */
export function stalenessMessage(binary, root, invocation) {
  const { binaryMtime, sourceMtime } = staleness(binary, root);
  const stem = path.basename(binary, path.extname(binary));
  return (
    `${binary} is older than the newest source (binary ${new Date(binaryMtime).toISOString()} ` +
    `< source ${new Date(sourceMtime).toISOString()}), so it does not contain the revision under ` +
    'test. The workspace target/debug tree is locked while an operator server runs out of it; ' +
    'build into a private target directory and point the runner at that copy:\n' +
    `  CARGO_TARGET_DIR=tmp/regress-target cargo build -p ${stem}\n` +
    `  CARGO_TARGET_DIR=tmp/regress-target ${invocation}`
  );
}
