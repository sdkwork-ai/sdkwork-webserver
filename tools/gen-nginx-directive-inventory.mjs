#!/usr/bin/env node
/**
 * Generate `specs/nginx-directive-inventory.json` from an unpacked nginx OSS
 * source tree and byte-compare the committed snapshot.
 *
 * The inventory is the machine-readable nginx 1.29.6 directive table required
 * by `docs/product/prd/PRD-nginx-compatibility.md` section 3: every directive
 * declared in an `ngx_command_t` table of `src/core`, `src/event`, `src/http`,
 * `src/stream`, and `src/mail`, with its allowed contexts, module namespace,
 * argument grammar, source file, and whether the declaring module is compiled
 * into the declared reference build. The directive list is parsed from the
 * real source tree; it is never hand-written. Third-party modules are not part
 * of a pristine nginx OSS tarball and are therefore absent by construction.
 *
 * The declared reference build is the nginx.org official 1.29.6 Linux package
 * (the pinned differential oracle for this repository). It enables the
 * optional SSL/HTTP2/HTTP3/stream/mail modules, so `referenceBuild` must be read
 * as "present in the reference build", not "present in a bare `./configure`".
 *
 * Usage:
 *   node tools/gen-nginx-directive-inventory.mjs --check
 *       Offline self-check of the committed file: canonical serialization,
 *       schema, digest, and cross-reference integrity. Used by the committed
 *       (network-free, source-free) gates.
 *
 *   node tools/gen-nginx-directive-inventory.mjs --check --source <dir>
 *       Full re-derivation from the unpacked source tree and byte comparison
 *       against the committed file. Requires bash (the nginx `auto/modules`
 *       module selection is executed with stub `. auto/module` / `. auto/have`
 *       scripts so the declared reference build configures exactly as nginx
 *       itself would).
 *
 *   node tools/gen-nginx-directive-inventory.mjs --write --source <dir>
 *       Regenerate the committed file.
 *
 * The source tarball is fetched once, by hand, from
 * https://nginx.org/download/nginx-1.29.6.tar.gz (sha256 pinned below);
 * this tool never uses the network.
 */
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '..');
const INVENTORY_PATH = path.join(REPO_ROOT, 'specs', 'nginx-directive-inventory.json');

export const NGINX_VERSION = '1.29.6';
export const NGINX_SOURCE_URL = `https://nginx.org/download/nginx-${NGINX_VERSION}.tar.gz`;
export const NGINX_SOURCE_SHA256 =
  '316f298cd9f061d6d0679696152710285b72f75d88eb1f7e323f40c5c52fe0d7';

/**
 * The declared reference build: nginx.org's official 1.29.6 Linux package.
 * `nginx -V` on that package reports these configure arguments (path/compiler
 * flags omitted; they do not select modules). This is the same build pinned as
 * the differential oracle in `specs/nginx-behavioral-corpus.manifest.json`
 * (REQ-2026-0071).
 */
export const REFERENCE_BUILD = {
  name: `nginx.org official ${NGINX_VERSION} Linux package`,
  platform: 'linux',
  configureArgs: [
    '--with-compat',
    '--with-file-aio',
    '--with-threads',
    '--with-http_addition_module',
    '--with-http_auth_request_module',
    '--with-http_dav_module',
    '--with-http_flv_module',
    '--with-http_gunzip_module',
    '--with-http_gzip_static_module',
    '--with-http_mp4_module',
    '--with-http_random_index_module',
    '--with-http_realip_module',
    '--with-http_secure_link_module',
    '--with-http_slice_module',
    '--with-http_ssl_module',
    '--with-http_stub_status_module',
    '--with-http_sub_module',
    '--with-http_v2_module',
    '--with-http_v3_module',
    '--with-mail',
    '--with-mail_ssl_module',
    '--with-stream',
    '--with-stream_realip_module',
    '--with-stream_ssl_module',
    '--with-stream_ssl_preread_module',
  ],
};

/** nginx module namespaces an inventory entry can belong to. */
export const GROUPS = ['core', 'events', 'http', 'stream', 'mail'];

const GROUP_BY_PREFIX = [
  ['src/http/', 'http'],
  ['src/stream/', 'stream'],
  ['src/mail/', 'mail'],
  ['src/event/', 'events'],
  ['src/core/', 'core'],
  ['src/os/', 'core'],
  ['src/misc/', 'core'],
];

export function groupOfSource(source) {
  for (const [prefix, group] of GROUP_BY_PREFIX) {
    if (source.startsWith(prefix)) return group;
  }
  return 'core';
}

/** Context bitmask macro names mapped to profile context labels, per group. */
const CONTEXT_MACROS = {
  core: { NGX_MAIN_CONF: 'main', NGX_DIRECT_CONF: 'main', NGX_ANY_CONF: 'any' },
  events: { NGX_EVENT_CONF: 'events', NGX_MAIN_CONF: 'main', NGX_ANY_CONF: 'any' },
  http: {
    NGX_HTTP_MAIN_CONF: 'http',
    NGX_HTTP_SRV_CONF: 'http-server',
    NGX_HTTP_LOC_CONF: 'http-location',
    NGX_HTTP_UPS_CONF: 'http-upstream',
    NGX_HTTP_SIF_CONF: 'http-if',
    NGX_HTTP_LIF_CONF: 'http-if',
    NGX_HTTP_LMT_CONF: 'http-limit-except',
    NGX_MAIN_CONF: 'main',
    NGX_ANY_CONF: 'any',
  },
  stream: {
    NGX_STREAM_MAIN_CONF: 'stream',
    NGX_STREAM_SRV_CONF: 'stream-server',
    NGX_STREAM_UPS_CONF: 'stream-upstream',
    NGX_MAIN_CONF: 'main',
    NGX_ANY_CONF: 'any',
  },
  mail: {
    NGX_MAIL_MAIN_CONF: 'mail',
    NGX_MAIL_SRV_CONF: 'mail-server',
    NGX_MAIN_CONF: 'main',
    NGX_ANY_CONF: 'any',
  },
};

/** `NGX_CONF_*` argument-grammar flag tokens normalized into a compact form. */
const ARITY_FLAGS = {
  NGX_CONF_NOARGS: 'no-args',
  NGX_CONF_FLAG: 'flag',
  NGX_CONF_TAKE1: '1',
  NGX_CONF_TAKE2: '2',
  NGX_CONF_TAKE3: '3',
  NGX_CONF_TAKE4: '4',
  NGX_CONF_TAKE5: '5',
  NGX_CONF_TAKE6: '6',
  NGX_CONF_TAKE7: '7',
  NGX_CONF_TAKE12: '1|2',
  NGX_CONF_TAKE13: '1|3',
  NGX_CONF_TAKE14: '1|4',
  NGX_CONF_TAKE15: '1|5',
  NGX_CONF_TAKE23: '2|3',
  NGX_CONF_TAKE24: '2|4',
  NGX_CONF_TAKE25: '2|5',
  NGX_CONF_TAKE34: '3|4',
  NGX_CONF_TAKE35: '3|5',
  NGX_CONF_TAKE45: '4|5',
  NGX_CONF_TAKE123: '1|2|3',
  NGX_CONF_TAKE1234: '1|2|3|4',
  NGX_CONF_TAKE124: '1|2|4',
  NGX_CONF_TAKE125: '1|2|5',
  NGX_CONF_TAKE134: '1|3|4',
  NGX_CONF_TAKE234: '2|3|4',
  NGX_CONF_1MORE: '1+',
  NGX_CONF_2MORE: '2+',
  NGX_CONF_ANY: 'any',
};

function stripComments(text) {
  let out = '';
  let i = 0;
  let state = 'code';
  while (i < text.length) {
    const c = text[i];
    const n = text[i + 1];
    if (state === 'code') {
      if (c === '/' && n === '*') {
        state = 'block';
        i += 2;
        continue;
      }
      if (c === '"') {
        out += c;
        state = 'string';
        i += 1;
        continue;
      }
      out += c;
      i += 1;
      continue;
    }
    if (state === 'block') {
      if (c === '*' && n === '/') {
        state = 'code';
        i += 2;
        continue;
      }
      if (c === '\n') out += '\n';
      i += 1;
      continue;
    }
    // string
    out += c;
    if (c === '\\') {
      out += n ?? '';
      i += 2;
      continue;
    }
    if (c === '"') state = 'code';
    i += 1;
  }
  return out;
}

function parseOptionsDefaults(optionsText) {
  const defaults = new Map();
  for (const line of optionsText.split('\n')) {
    const match = /^([A-Z][A-Z0-9_]*)=(.*)$/u.exec(line);
    if (!match) continue;
    const [, name, rawValue] = match;
    if (/[$`]|\$\(/u.test(rawValue)) continue;
    if (rawValue === '""' || rawValue === "''") continue;
    defaults.set(name, rawValue.trim());
  }
  return defaults;
}

function parseOptionHandlers(optionsText) {
  const handlers = new Map();
  const lines = optionsText.split('\n');
  for (let i = 0; i < lines.length; i += 1) {
    const match = /^\s*(--[A-Za-z0-9_.=+-]+)\)/u.exec(lines[i]);
    if (!match) continue;
    // nginx's `case` arms may put the body on the next line, so accumulate
    // until the terminating `;;`.
    let body = lines[i].slice(match.index + match[0].length);
    let cursor = i;
    while (!body.includes(';;') && cursor + 1 < lines.length) {
      cursor += 1;
      body += `\n${lines[cursor]}`;
    }
    const assignments = [];
    for (const part of body.split(';')) {
      const assignment = /^\s*([A-Z][A-Z0-9_]*)=(\S+)\s*$/u.exec(part);
      if (assignment) assignments.push([assignment[1], assignment[2]]);
    }
    if (assignments.length > 0) handlers.set(match[1], assignments);
    i = cursor;
  }
  return handlers;
}

/** Parse `auto/sources` variable assignments (`NAME="a b c"` with continuations). */
function parseAutoSources(sourcesText) {
  const variables = new Map();
  const re = /^([A-Z][A-Z0-9_]+)="((?:[^"\\]|\\.)*)"/gmsu;
  let match;
  while ((match = re.exec(sourcesText))) {
    variables.set(match[1], match[2].replace(/\\\s*\n\s*/gu, ' '));
  }
  // Unquoted one-token assignments (`EPOLL_MODULE=ngx_epoll_module`).
  for (const line of sourcesText.split('\n')) {
    const simple = /^([A-Z][A-Z0-9_]+)=([^\s"$`]+)\s*$/u.exec(line);
    if (simple && !variables.has(simple[1])) variables.set(simple[1], simple[2]);
  }
  return variables;
}

/** Read the `--with-*` feature variables selected by the reference build. */
function referenceBuildVariables(sourceDir) {
  const optionsText = fs.readFileSync(path.join(sourceDir, 'auto', 'options'), 'utf8');
  const defaults = parseOptionsDefaults(optionsText);
  const handlers = parseOptionHandlers(optionsText);
  const vars = new Map(defaults);
  const unknownOptions = [];
  for (const option of REFERENCE_BUILD.configureArgs) {
    const assignments = handlers.get(option);
    if (!assignments) {
      unknownOptions.push(option);
      continue;
    }
    for (const [name, value] of assignments) vars.set(name, value);
  }
  if (unknownOptions.length > 0) {
    throw new Error(
      `reference build configure argument(s) not understood by auto/options: ${unknownOptions.join(', ')}`,
    );
  }
  // Configure enables a POSIX event module on Linux; mirror `auto/os/linux`.
  const autoSources = parseAutoSources(fs.readFileSync(path.join(sourceDir, 'auto', 'sources'), 'utf8'));
  const osLinux = fs.readFileSync(path.join(sourceDir, 'auto', 'os', 'linux'), 'utf8');
  const eventModules = new Set((autoSources.get('EVENT_MODULES') ?? '').split(/\s+/u).filter(Boolean));
  for (const match of osLinux.matchAll(/EVENT_MODULES="\$EVENT_MODULES \$([A-Z0-9_]+)"/gu)) {
    const moduleName = autoSources.get(`${match[1]}_MODULE`);
    if (moduleName) {
      for (const name of moduleName.split(/\s+/u).filter(Boolean)) eventModules.add(name);
    }
  }
  if (osLinux.includes('have=NGX_LINUX')) vars.set('NGX_LINUX', 'YES');
  return { vars, autoSources, eventModules };
}

/**
 * Execute nginx's own `auto/modules` shell module selection with stub
 * `. auto/module` / `. auto/have` bodies. This is exactly the selection nginx's
 * configure performs; the stubs only record the result instead of writing
 * `objs/ngx_modules.c` and `objs/ngx_auto_config.h`.
 */
function simulateModules(sourceDir, vars) {
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'sdkwork-nginx-inventory-'));
  try {
    fs.mkdirSync(path.join(scratch, 'auto'));
    fs.copyFileSync(
      path.join(sourceDir, 'auto', 'modules'),
      path.join(scratch, 'auto', 'modules'),
    );
    fs.writeFileSync(
      path.join(scratch, 'auto', 'have'),
      '#!/bin/sh\nprintf "%s\\n" "$have" >> "$NGXSIM_MACROS"\n',
    );
    fs.writeFileSync(
      path.join(scratch, 'auto', 'module'),
      '#!/bin/sh\nprintf "%s|%s|%s|%s\\n" "$ngx_module_type" "$(echo $ngx_module_name)" '
        + '"$(echo $ngx_module_srcs)" "$ngx_module_link" >> "$NGXSIM_MODULES"\n',
    );
    fs.chmodSync(path.join(scratch, 'auto', 'have'), 0o755);
    fs.chmodSync(path.join(scratch, 'auto', 'module'), 0o755);
    const shellQuote = (value) => `'${String(value).replace(/'/gu, `'\\''`)}'`;
    const lines = ['set -e', 'NGXSIM_MACROS="$PWD/macros.txt"', 'NGXSIM_MODULES="$PWD/modules.txt"',
      'NGX_MODULES_C="$PWD/ngx_modules.c"', ': > "$NGXSIM_MACROS"', ': > "$NGXSIM_MODULES"'];
    for (const [name, value] of [...vars.entries()].sort(([a], [b]) => a.localeCompare(b))) {
      if (!/^[A-Z][A-Z0-9_]*$/u.test(name) || /[$`]/u.test(value)) continue;
      lines.push(`${name}=${shellQuote(value)}`);
    }
    lines.push('. ./auto/modules');
    fs.writeFileSync(path.join(scratch, 'run.sh'), `${lines.join('\n')}\n`);
    execFileSync('sh', [path.join(scratch, 'run.sh')], { cwd: scratch, stdio: 'pipe' });
    const macros = new Set(
      fs.readFileSync(path.join(scratch, 'macros.txt'), 'utf8')
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => /^[A-Z][A-Z0-9_]*$/u.test(line)),
    );
    const modules = [];
    for (const line of fs.readFileSync(path.join(scratch, 'modules.txt'), 'utf8').split('\n')) {
      if (!line.trim()) continue;
      const [type, names, srcs, link] = line.split('|');
      modules.push({
        type: (type ?? '').trim(),
        names: (names ?? '').trim().split(/\s+/u).filter(Boolean),
        srcs: (srcs ?? '').trim().split(/\s+/u).filter((src) => src.endsWith('.c')),
        link: (link ?? '').trim(),
      });
    }
    const modulesC = fs.readFileSync(path.join(scratch, 'ngx_modules.c'), 'utf8');
    return { macros, modules, modulesC };
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

/** Platform feature macros of the declared reference platform (Ubuntu Linux). */
function platformMacros(sourceDir, vars) {
  const macros = new Set(['NGX_LINUX']);
  for (const file of ['auto/os/linux', 'auto/unix']) {
    const text = fs.readFileSync(path.join(sourceDir, file), 'utf8');
    for (const match of text.matchAll(/ngx_feature_name="([A-Z][A-Z0-9_]*)"/gu)) {
      macros.add(match[1]);
    }
    for (const match of text.matchAll(/have=([A-Z][A-Z0-9_]*) \. auto\/have/gu)) {
      macros.add(match[1]);
    }
  }
  // Feature probes gated on configure variables: absent when the reference
  // build does not select them.
  if (vars.get('NGX_FILE_AIO') !== 'YES') macros.delete('NGX_HAVE_FILE_AIO');
  return macros;
}

function fileIsCompiled(sourceDir, moduleSelection) {
  const compiled = new Set();
  const { autoSources } = moduleSelection;
  const addList = (value) => {
    for (const src of (value ?? '').split(/\s+/u)) if (src.endsWith('.c')) compiled.add(src);
  };
  for (const module of moduleSelection.modules) {
    if (module.link === 'YES') for (const src of module.srcs) compiled.add(src);
  }
  addList(autoSources.get('CORE_SRCS'));
  addList(autoSources.get('EVENT_SRCS'));
  if (moduleSelection.vars.get('USE_THREADS') === 'YES') {
    addList(autoSources.get('THREAD_POOL_SRCS'));
  }
  const osLinux = fs.readFileSync(path.join(sourceDir, 'auto/os/linux'), 'utf8');
  for (const match of osLinux.matchAll(/\$([A-Z0-9_]+)_SRCS/gu)) {
    addList(autoSources.get(`${match[1]}_SRCS`));
  }
  for (const match of osLinux.matchAll(/([A-Z][A-Z0-9_]*)_SRCS/gu)) {
    addList(autoSources.get(`${match[1]}_SRCS`));
  }
  return compiled;
}

function collectSourceFiles(sourceDir, compiled) {
  const files = new Set(compiled);
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else if (entry.name.endsWith('.c')) {
        files.add(path.relative(sourceDir, full).split(path.sep).join('/'));
      }
    }
  };
  walk(path.join(sourceDir, 'src'));
  return [...files].sort();
}

const TRUE_EXPR = /^[A-Za-z_][A-Za-z0-9_]*$/u;

/**
 * Evaluate a `#if` expression against known macros. `unknownTrue` decides the
 * value of a macro the reference build declarations do not mention: on the
 * declared Linux reference platform an unlisted `NGX_HAVE_*` probe exists,
 * while other unknown macros (debug variants) do not.
 */
function evalCondition(expression, isKnown, isSet, unknownTrue) {
  const tokens = expression
    .replace(/defined\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)/gu, 'defined($1)')
    .trim();
  const orParts = splitTop(tokens, '||');
  if (orParts.length > 1) return orParts.some((part) => evalCondition(part, isKnown, isSet, unknownTrue));
  const andParts = splitTop(orParts[0], '&&');
  if (andParts.length > 1) return andParts.every((part) => evalCondition(part, isKnown, isSet, unknownTrue));
  let value = andParts[0].trim();
  let negate = false;
  while (value.startsWith('!')) {
    negate = !negate;
    value = value.slice(1).trim();
  }
  while (value.startsWith('(') && value.endsWith(')')) value = value.slice(1, -1).trim();
  if (/^\d+$/u.test(value)) {
    const result = Number(value) !== 0;
    return negate ? !result : result;
  }
  const defined = /^defined\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)$/u.exec(value);
  if (defined) {
    const present = isKnown(defined[1]);
    return negate ? !present : present;
  }
  if (TRUE_EXPR.test(value)) {
    const result = isKnown(value) && isSet(value)
      ? true
      : (!isKnown(value) && unknownTrue ? true : false);
    return negate ? !result : result;
  }
  return false;
}

function splitTop(expression, operator) {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < expression.length; i += 1) {
    const c = expression[i];
    if (c === '(') depth += 1;
    else if (c === ')') depth -= 1;
    else if (depth === 0 && expression.startsWith(operator, i)) {
      parts.push(expression.slice(start, i));
      i += operator.length - 1;
      start = i + 1;
    }
  }
  parts.push(expression.slice(start));
  return parts;
}

/**
 * Apply C preprocessor conditionals to a file and return only the lines that
 * the reference build compiles.
 */
function preprocess(sourceText, isKnown, isSet) {
  const lines = stripComments(sourceText).split('\n');
  const stack = [];
  const output = [];
  let active = true;
  const unknownTrue = true;
  for (const line of lines) {
    const trimmed = line.trim();
    let match;
    if ((match = /^#\s*ifdef\s+([A-Za-z_][A-Za-z0-9_]*)/u.exec(trimmed))) {
      const condition = isKnown(match[1]) && isSet(match[1]);
      stack.push({ parent: active, taken: condition });
      active = active && condition;
      continue;
    }
    if ((match = /^#\s*ifndef\s+([A-Za-z_][A-Za-z0-9_]*)/u.exec(trimmed))) {
      const condition = !(isKnown(match[1]) && isSet(match[1]));
      stack.push({ parent: active, taken: condition });
      active = active && condition;
      continue;
    }
    if ((match = /^#\s*if\s+(.+)$/u.exec(trimmed))) {
      const condition = evalCondition(match[1], isKnown, isSet, unknownTrue);
      stack.push({ parent: active, taken: condition });
      active = active && condition;
      continue;
    }
    if ((match = /^#\s*elif\s+(.+)$/u.exec(trimmed))) {
      const frame = stack[stack.length - 1];
      if (!frame) throw new Error('unbalanced #elif');
      const condition = !frame.taken && evalCondition(match[1], isKnown, isSet, unknownTrue);
      frame.taken = frame.taken || condition;
      active = frame.parent && condition;
      continue;
    }
    if (/^#\s*else\b/u.test(trimmed)) {
      const frame = stack[stack.length - 1];
      if (!frame) throw new Error('unbalanced #else');
      const condition = !frame.taken;
      frame.taken = frame.taken || condition;
      active = frame.parent && condition;
      continue;
    }
    if (/^#\s*endif\b/u.test(trimmed)) {
      stack.pop();
      active = stack.length > 0 ? stack[stack.length - 1].parent && stack[stack.length - 1].taken : true;
      continue;
    }
    if (active) output.push(line);
  }
  return output.join('\n');
}

function splitTopLevelCommas(body) {
  const fields = [];
  let depth = 0;
  let inString = false;
  let current = '';
  for (let i = 0; i < body.length; i += 1) {
    const c = body[i];
    if (inString) {
      current += c;
      if (c === '\\') {
        current += body[i + 1] ?? '';
        i += 1;
      } else if (c === '"') {
        inString = false;
      }
      continue;
    }
    if (c === '"') {
      inString = true;
      current += c;
      continue;
    }
    if (c === '(') depth += 1;
    if (c === ')') depth -= 1;
    if (c === ',' && depth === 0) {
      fields.push(current.trim());
      current = '';
      continue;
    }
    current += c;
  }
  if (current.trim()) fields.push(current.trim());
  return fields;
}

/** Extract every directive declared by an `ngx_command_t <name>[] = { ... }`. */
function parseCommandTables(compiledText, source) {
  const entries = [];
  for (const match of compiledText.matchAll(/(?:static\s+)?ngx_command_t\s+(\w+)\s*\[\s*\]\s*=\s*\{/gu)) {
    const tableName = match[1];
    let index = match.index + match[0].length;
    let depth = 1;
    const start = index;
    while (index < compiledText.length && depth > 0) {
      const c = compiledText[index];
      if (c === '{') depth += 1;
      else if (c === '}') depth -= 1;
      index += 1;
    }
    const body = compiledText.slice(start, index - 1);
    let cursor = 0;
    while (cursor < body.length) {
      const open = body.indexOf('{', cursor);
      if (open === -1) break;
      let nested = 1;
      let scan = open + 1;
      let inString = false;
      while (scan < body.length && nested > 0) {
        const c = body[scan];
        if (inString) {
          if (c === '\\') scan += 1;
          else if (c === '"') inString = false;
        } else if (c === '"') inString = true;
        else if (c === '{') nested += 1;
        else if (c === '}') nested -= 1;
        scan += 1;
      }
      const entry = body.slice(open + 1, scan - 1);
      cursor = scan;
      const nameMatch = /ngx_string\s*\(\s*"([^"]+)"\s*\)/u.exec(entry);
      if (!nameMatch) continue;
      const fields = splitTopLevelCommas(entry);
      entries.push({ table: tableName, name: nameMatch[1], flags: fields[1] ?? '' });
    }
  }
  return entries.map((entry) => ({ ...entry, source }));
}

function contextLabels(group, flags) {
  const macroMap = CONTEXT_MACROS[group] ?? CONTEXT_MACROS.core;
  const contexts = new Set();
  for (const [macro, label] of Object.entries(macroMap)) {
    if (new RegExp(`\\b${macro}\\b`, 'u').test(flags)) contexts.add(label);
  }
  return [...contexts].sort();
}

function argumentGrammar(flags) {
  const found = new Set();
  for (const token of Object.keys(ARITY_FLAGS)) {
    if (new RegExp(`\\b${token}\\b`, 'u').test(flags)) found.add(ARITY_FLAGS[token]);
  }
  if (found.size === 0) return null;
  return [...found].sort().join('|');
}

function isBlock(flags) {
  return /\bNGX_CONF_BLOCK\b/u.test(flags);
}

function modulesDeclaredIn(sourceText) {
  const names = [];
  for (const match of sourceText.matchAll(/ngx_module_t\s+(\w+)\s*=/gu)) names.push(match[1]);
  return names;
}

export function deriveInventory(sourceDir) {
  const moduleSelection = referenceBuildVariables(sourceDir);
  const simulation = simulateModules(sourceDir, moduleSelection.vars);
  const compiled = fileIsCompiled(sourceDir, { ...moduleSelection, modules: simulation.modules });
  // Feature macros selected by auto/modules plus OS/platform probes.
  const known = new Set([...simulation.macros, ...platformMacros(sourceDir, moduleSelection.vars)]);
  const isKnown = (name) => known.has(name);
  const isSet = (name) => known.has(name);
  const allSources = collectSourceFiles(sourceDir, compiled);
  const byId = new Map();
  for (const source of allSources) {
    const raw = fs.readFileSync(path.join(sourceDir, source), 'utf8');
    const referenceBuild = compiled.has(source);
    // Compiled files are preprocessed exactly as the reference build sees
    // them; non-compiled module files contribute their declared directives
    // with referenceBuild=false so the inventory is the whole OSS table.
    const text = referenceBuild ? preprocess(raw, isKnown, isSet) : stripComments(raw);
    const modules = modulesDeclaredIn(raw);
    const group = groupOfSource(source);
    for (const entry of parseCommandTables(text, source)) {
      const id = `${group}:${entry.name}`;
      if (!byId.has(id)) {
        byId.set(id, {
          id,
          name: entry.name,
          group,
          modules: new Set(),
          contexts: new Set(),
          grammar: new Set(),
          block: false,
          referenceBuild: false,
          sources: new Set(),
        });
      }
      const record = byId.get(id);
      for (const moduleName of modules) record.modules.add(moduleName);
      for (const context of contextLabels(group, entry.flags)) record.contexts.add(context);
      const grammar = argumentGrammar(entry.flags);
      if (grammar) record.grammar.add(grammar);
      record.block = record.block || isBlock(entry.flags);
      record.referenceBuild = record.referenceBuild || referenceBuild;
      record.sources.add(entry.source);
    }
  }
  const directives = [...byId.values()]
    .map((record) => ({
      id: record.id,
      name: record.name,
      group: record.group,
      modules: [...record.modules].sort(),
      contexts: [...record.contexts].sort(),
      grammar: [...record.grammar].sort().join('|') || null,
      block: record.block,
      referenceBuild: record.referenceBuild,
      sources: [...record.sources].sort(),
    }))
    .sort((a, b) => a.id.localeCompare(b.id));
  const payload = {
    schemaVersion: 1,
    kind: 'sdkwork.webserver.nginx-directive-inventory',
    source: {
      vendor: 'nginx',
      product: 'nginx OSS',
      version: NGINX_VERSION,
      url: NGINX_SOURCE_URL,
      sha256: NGINX_SOURCE_SHA256,
      license: 'BSD-2-Clause',
      generator: 'tools/gen-nginx-directive-inventory.mjs',
      thirdPartyModules: 'excluded (a pristine nginx OSS tarball contains none)',
    },
    referenceBuild: {
      ...REFERENCE_BUILD,
      note:
        'The nginx.org official package is the pinned differential oracle. A bare '
        + '`./configure` does not build ssl/http2/http3/stream/mail, so referenceBuild '
        + 'means "compiled into the declared reference build", not "present in a default build".',
    },
    directiveCount: directives.length,
    directives,
  };
  payload.digest = inventoryDigest(payload);
  return payload;
}

export function inventoryDigest(payload) {
  const clone = structuredClone(payload);
  delete clone.digest;
  return crypto.createHash('sha256').update(JSON.stringify(clone)).digest('hex');
}

export function serializeInventory(payload) {
  return `${JSON.stringify(payload, null, 2)}\n`;
}

function validateCommitted(inventoryText) {
  const inventory = JSON.parse(inventoryText);
  const problems = [];
  if (inventory.schemaVersion !== 1) problems.push('schemaVersion must be 1');
  if (inventory.kind !== 'sdkwork.webserver.nginx-directive-inventory') {
    problems.push('unexpected kind');
  }
  if (inventory.source?.version !== NGINX_VERSION) problems.push('source version mismatch');
  if (inventory.source?.url !== NGINX_SOURCE_URL) problems.push('source url mismatch');
  if (inventory.source?.sha256 !== NGINX_SOURCE_SHA256) problems.push('source sha256 mismatch');
  if (!Array.isArray(inventory.directives) || inventory.directives.length === 0) {
    problems.push('directives must be a non-empty array');
  } else if (inventory.directiveCount !== inventory.directives.length) {
    problems.push('directiveCount must equal directives.length');
  }
  const ids = new Set();
  for (const directive of inventory.directives ?? []) {
    if (!directive.id || ids.has(directive.id)) problems.push(`duplicate or missing id: ${directive.id}`);
    ids.add(directive.id);
    if (!GROUPS.includes(directive.group)) problems.push(`${directive.id}: bad group`);
    if (!Array.isArray(directive.contexts) || directive.contexts.length === 0) {
      problems.push(`${directive.id}: missing contexts`);
    }
    if (!Array.isArray(directive.sources) || directive.sources.length === 0) {
      problems.push(`${directive.id}: missing sources`);
    }
  }
  if (inventory.digest !== inventoryDigest(inventory)) problems.push('digest mismatch');
  const canonical = serializeInventory(inventory);
  if (canonical !== inventoryText) problems.push('file is not in canonical serialized form');
  return { inventory, problems };
}

function parseArgs(argv) {
  const args = { source: null, check: false, write: false, help: false };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--source') args.source = path.resolve(argv[++i]);
    else if (arg === '--check') args.check = true;
    else if (arg === '--write') args.write = true;
    else if (arg === '--help' || arg === '-h') args.help = true;
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (!args.check && !args.write && !args.help) args.check = true;
  return args;
}

function main() {
  const args = parseArgs(process.argv);
  if (args.help) {
    process.stdout.write(
      'Usage: node tools/gen-nginx-directive-inventory.mjs [--check] [--write] [--source <unpacked-nginx-source-dir>]\n',
    );
    return;
  }
  if (args.source) {
    const derived = deriveInventory(args.source);
    const derivedText = serializeInventory(derived);
    if (args.write) {
      fs.writeFileSync(INVENTORY_PATH, derivedText);
      process.stdout.write(
        `wrote specs/nginx-directive-inventory.json (${derived.directiveCount} directives)\n`,
      );
      return;
    }
    const committed = fs.readFileSync(INVENTORY_PATH, 'utf8');
    if (committed !== derivedText) {
      const a = committed.split('\n');
      const b = derivedText.split('\n');
      let line = 0;
      while (line < Math.max(a.length, b.length) && a[line] === b[line]) line += 1;
      throw new Error(
        `specs/nginx-directive-inventory.json is out of date relative to the nginx ${NGINX_VERSION} source tree; `
          + `first difference at line ${line + 1}:\n  committed: ${a[line] ?? '<eof>'}\n  derived:   ${b[line] ?? '<eof>'}`,
      );
    }
    const { problems } = validateCommitted(committed);
    if (problems.length > 0) throw new Error(problems.join('\n'));
    process.stdout.write(
      `nginx directive inventory matches the nginx ${NGINX_VERSION} source tree `
        + `(${derived.directiveCount} directives)\n`,
    );
    return;
  }
  const committed = fs.readFileSync(INVENTORY_PATH, 'utf8');
  const { inventory, problems } = validateCommitted(committed);
  if (problems.length > 0) throw new Error(problems.join('\n'));
  process.stdout.write(
    `nginx directive inventory is canonical and valid (${inventory.directiveCount} directives, `
      + `nginx ${NGINX_VERSION}); pass --source <dir> to re-derive it from the tarball\n`,
  );
}

const isDirectRun = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isDirectRun) {
  try {
    main();
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exit(1);
  }
}
