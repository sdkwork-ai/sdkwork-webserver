#!/usr/bin/env node
/**
 * 跨仓数据库前缀归属门禁（DATABASE_SPEC §7）。
 *
 * 检查四类缺陷：
 *   F1 UNREGISTERED_PREFIX —— 某仓声明了工作区模块注册表里没有的前缀
 *   F2 PREFIX_COLLISION    —— 同一前缀被 ≥2 个仓声明（命名空间撞车）
 *   F3 OWNER_MISMATCH      —— 某仓声明的前缀在注册表里归属另一个仓
 *   F4 TABLE_COLLISION     —— 同一张表被 ≥2 个仓的 table-registry 声明
 *
 * 用法：
 *   node scripts/check-database-prefix-ownership.mjs [--workspace D:/sdkwork-space] [--json]
 *
 * 退出码：0 = 无发现；1 = 有发现（可作为门禁）。
 */

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

function parseArgs(argv) {
  // 默认工作区 = 本仓所在目录（<workspace>/sdkwork-webserver/scripts → <workspace>）
  const defaultWorkspace = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
  const args = { workspace: defaultWorkspace, json: false };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === '--workspace') args.workspace = argv[++i];
    else if (argv[i] === '--json') args.json = true;
  }
  return args;
}

const readJson = (p) => JSON.parse(readFileSync(p, 'utf8'));

/** 只扫描一级子目录里的 git 仓，避免爬进 node_modules / target。 */
function listRepos(workspace) {
  return readdirSync(workspace)
    .map((name) => path.join(workspace, name))
    .filter((p) => {
      try {
        return statSync(p).isDirectory() && existsSync(path.join(p, '.git'));
      } catch {
        return false;
      }
    });
}

/** 表名 -> 前缀：取该仓已声明前缀里最长的匹配，避免 web_ 吃掉 webstore_。 */
function prefixOf(tableName, declaredPrefixes) {
  const matches = declaredPrefixes.filter((p) => tableName.startsWith(p));
  return matches.sort((a, b) => b.length - a.length)[0] ?? null;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const registryPath = path.join(args.workspace, 'sdkwork-specs/tools/database-module-registry.json');
  if (!existsSync(registryPath)) {
    console.error(`workspace module registry not found: ${registryPath}`);
    process.exit(2);
  }
  const registryDoc = readJson(registryPath);
  // 注册表根节点是数组（32 条条目），不是对象。
  const registryEntries = Array.isArray(registryDoc)
    ? registryDoc
    : (registryDoc.modules ?? registryDoc.prefixes ?? []);
  const registeredPrefix = new Map(registryEntries.map((e) => [e.tablePrefix, e]));

  const claims = new Map(); // prefix -> [{repo, owner, domain}]
  const tableClaims = new Map(); // table -> [repo]
  const repoPrefixes = new Map(); // repo -> [prefix]

  for (const repo of listRepos(args.workspace)) {
    const repoName = path.basename(repo);
    const prefixPath = path.join(repo, 'database/contract/prefix-registry.json');
    const tablePath = path.join(repo, 'database/contract/table-registry.json');
    const prefixes = existsSync(prefixPath) ? (readJson(prefixPath).prefixes ?? []) : [];
    if (prefixes.length > 0) {
      repoPrefixes.set(repoName, prefixes.map((p) => p.prefix));
      for (const p of prefixes) {
        if (!claims.has(p.prefix)) claims.set(p.prefix, []);
        claims.get(p.prefix).push({ repo: repoName, owner: p.owner, domain: p.domain });
      }
    }
    if (existsSync(tablePath)) {
      const declared = prefixes.map((p) => p.prefix);
      for (const t of readJson(tablePath).tables ?? []) {
        // 只对「本仓声明了前缀」或「表名不属他仓」的情况记账
        if (declared.length > 0 && prefixOf(t.table_name, declared) === null) continue;
        if (!tableClaims.has(t.table_name)) tableClaims.set(t.table_name, []);
        tableClaims.get(t.table_name).push(repoName);
      }
    }
  }

  const findings = [];

  for (const [prefix, claimants] of claims) {
    const uniqueRepos = [...new Set(claimants.map((c) => c.repo))];
    if (uniqueRepos.length > 1) {
      findings.push({
        code: 'F2_PREFIX_COLLISION',
        prefix,
        detail: `claimed by ${uniqueRepos.length} repos: ${uniqueRepos.join(', ')}`,
        repos: uniqueRepos,
      });
    }
    const registered = registeredPrefix.get(prefix);
    if (!registered) {
      findings.push({
        code: 'F1_UNREGISTERED_PREFIX',
        prefix,
        detail: `not present in database-module-registry.json (${registeredPrefix.size} entries); claimed by ${uniqueRepos.join(', ')}`,
        repos: uniqueRepos,
      });
    } else {
      for (const repo of uniqueRepos) {
        if (repo !== registered.repo) {
          findings.push({
            code: 'F3_OWNER_MISMATCH',
            prefix,
            detail: `repo ${repo} claims it, but the registry assigns ${prefix} to ${registered.repo} (moduleId=${registered.moduleId})`,
            repos: [repo, registered.repo],
          });
        }
      }
    }
  }

  for (const [table, repos] of tableClaims) {
    const unique = [...new Set(repos)];
    if (unique.length > 1) {
      findings.push({
        code: 'F4_TABLE_COLLISION',
        table,
        detail: `declared by ${unique.length} repos: ${unique.join(', ')}`,
        repos: unique,
      });
    }
  }

  const summary = {
    workspace: args.workspace,
    registeredPrefixes: registeredPrefix.size,
    reposWithPrefixRegistry: repoPrefixes.size,
    findings,
  };

  if (args.json) {
    console.log(JSON.stringify(summary, null, 2));
  } else {
    console.log(`workspace            : ${args.workspace}`);
    console.log(`registered prefixes  : ${registeredPrefix.size}`);
    console.log(`repos declaring prefix: ${repoPrefixes.size}`);
    console.log(`findings             : ${findings.length}`);
    for (const f of findings) console.log(`  [${f.code}] ${f.prefix ?? f.table} — ${f.detail}`);
  }
  process.exit(findings.length === 0 ? 0 : 1);
}

main();
