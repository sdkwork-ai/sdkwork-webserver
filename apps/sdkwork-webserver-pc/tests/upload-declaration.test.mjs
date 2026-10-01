/**
 * Application upload declaration conformance (`DRIVE_SPEC.md` §18).
 *
 * The declaration file is the authority; the constants module carries its values into code so
 * call sites do not repeat literals. This suite keeps the two from drifting: the exact failure
 * class the `2b5357de` truthfulness fix addressed — a constant diverging from the declaration —
 * fails here instead of producing an upload statistic whose declared value and sent value
 * disagree.
 *
 * The constants module is TypeScript and this suite runs under vitest, so the module is read and
 * pattern-matched rather than imported. The assertion target is the real file on disk, so a
 * divergence still fails here; a value that cannot be read back fails too, by design.
 */
import { describe, expect, it } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const appRoot = path.resolve(import.meta.dirname, '..');
const DECLARATION_PATH = path.join(appRoot, 'specs/upload.declaration.json');
const CONSTANTS_PATH = path.join(
  appRoot,
  'packages/sdkwork-webserver-pc-console-core/src/sdk/uploadDeclaration.ts',
);

/** §8.1 standard upload profiles. A profile outside this set is a contract violation. */
const STANDARD_UPLOAD_PROFILES = new Set([
  'generic',
  'video',
  'image',
  'audio',
  'document',
  'archive',
  'text',
  'dataset',
  'attachment',
  'avatar',
  'thumbnail',
]);

/** §9.4 reserves `im` for Drive; an application must not declare or send it. */
const RESERVED_SCENES = new Set(['im']);

const KEBAB_CASE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const APP_RESOURCE_TYPE = /^[a-z][a-z0-9]*(?:\.[a-z][a-z0-9_]*)+$/;

function loadDeclaration() {
  return JSON.parse(fs.readFileSync(DECLARATION_PATH, 'utf8'));
}

/**
 * Read the exported upload entry objects out of the constants module.
 *
 * A word boundary is written as `(?:^|[^A-Za-z0-9_])` rather than `\b`: the pattern is built
 * with `new RegExp` from a template string, and `\b` survives one escaping layer too few as a
 * backspace character, which silently matches nothing.
 */
function readDeclaredConstantValues(source) {
  const stringConstants = new Map();
  for (const match of source.matchAll(
    /(?:^|[^A-Za-z0-9_])export\s+const\s+([A-Z0-9_]+)(?:\s*:[^=]+)?\s*=\s*['"]([^'"]+)['"]\s*as const/g,
  )) {
    stringConstants.set(match[1], match[2]);
  }
  const resolve = (raw) => {
    if (raw === undefined) {
      return undefined;
    }
    const literal = raw.match(/^['"]([^'"]+)['"]$/);
    if (literal) {
      return literal[1];
    }
    const expression = raw.trim();
    if (stringConstants.has(expression)) {
      return stringConstants.get(expression);
    }
    return undefined;
  };

  const values = [];
  const entryPattern =
    /(?:^|[^A-Za-z0-9_])export const [A-Z0-9_]*UPLOAD[A-Z0-9_]*\s*=\s*\{/g;
  const blocks = source.split(entryPattern);
  for (const block of blocks.slice(1)) {
    const end = block.indexOf('} as const');
    const body = end >= 0 ? block.slice(0, end) : block.slice(0, 800);
    const pick = (key) =>
      resolve(
        body.match(
          new RegExp('(?:^|[^A-Za-z0-9_])' + key + '\\s*:\\s*([\'"][^\'"]+[\'"]|[A-Za-z0-9_]+)'),
        )?.[1],
      );
    values.push({
      appResourceIdKind: pick('appResourceIdKind'),
      appResourceType: pick('appResourceType'),
      purpose: pick('purpose'),
      retention: pick('retention'),
      scene: pick('scene'),
      source: pick('source'),
      uploadProfileCode: pick('uploadProfileCode'),
    });
  }
  return values;
}

describe('upload declaration conformance', () => {
  it('declaration file exists, parses, and uses the supported schema', () => {
    const declaration = loadDeclaration();
    expect(declaration.schemaVersion).toBe(1);
    expect(Array.isArray(declaration.declarations)).toBe(true);
    expect(declaration.declarations.length).toBeGreaterThan(0);
  });

  it('declaration declares every required field on every entry', () => {
    const required = [
      'appResourceType',
      'appResourceIdKind',
      'scene',
      'source',
      'uploadProfileCode',
      'retention',
      'purpose',
    ];
    for (const entry of loadDeclaration().declarations) {
      for (const field of required) {
        expect(entry[field], `entry ${entry.appResourceType} is missing ${field}`).toBeTruthy();
      }
    }
  });

  it('declaration uses standard upload profiles only', () => {
    for (const entry of loadDeclaration().declarations) {
      expect(
        STANDARD_UPLOAD_PROFILES.has(entry.uploadProfileCode),
        `${entry.appResourceType} declares a non-standard profile ${entry.uploadProfileCode}`,
      ).toBe(true);
    }
  });

  it('declaration names appResourceType as a dotted lowercase business type', () => {
    for (const entry of loadDeclaration().declarations) {
      expect(entry.appResourceType).toMatch(APP_RESOURCE_TYPE);
    }
  });

  it('declaration names source and scene as stable lowercase kebab-case labels', () => {
    for (const entry of loadDeclaration().declarations) {
      // A package name, npm specifier, or import path is forbidden as `source`.
      expect(entry.source).toMatch(KEBAB_CASE);
      expect(entry.source.includes('/'), `${entry.source} contains a path separator`).toBe(false);
      expect(entry.source.includes('@'), `${entry.source} contains an npm scope`).toBe(false);
      expect(entry.scene).toMatch(KEBAB_CASE);
      expect(
        RESERVED_SCENES.has(entry.scene),
        `${entry.scene} is a scene reserved for Drive (§9.4)`,
      ).toBe(false);
    }
  });

  it('declaration appId matches the application config', () => {
    // §18.4: the declaration must match the config, not merely a local constant.
    const config = JSON.parse(
      fs.readFileSync(path.join(appRoot, 'sdkwork.app.config.json'), 'utf8'),
    );
    const appId = config.backend?.appId ?? config.app?.key;
    expect(appId, 'sdkwork.app.config.json does not declare an app identity').toBeTruthy();
    expect(loadDeclaration().appId).toBe(appId);
  });

  it('constants mirror the declaration file field for field', () => {
    const constants = readDeclaredConstantValues(fs.readFileSync(CONSTANTS_PATH, 'utf8'));
    const declared = loadDeclaration().declarations;
    expect(constants.length, 'constant count differs from declaration count').toBe(
      declared.length,
    );
    declared.forEach((declaredEntry, index) => {
      const constant = constants[index];
      const where = `entry #${index + 1} (${declaredEntry.appResourceType} / ${declaredEntry.scene})`;
      for (const field of [
        'appResourceIdKind',
        'appResourceType',
        'purpose',
        'retention',
        'scene',
        'source',
        'uploadProfileCode',
      ]) {
        expect(constant[field], `constant ${field} disagrees with the declaration for ${where}`).toBe(
          declaredEntry[field],
        );
      }
    });
  });
});
