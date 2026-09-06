#!/usr/bin/env node
// Remove stale memory*/mem* host claims from sdkwork-llm topology (v2, prefix match).
import fs from 'node:fs';
import path from 'node:path';

const llmRoot = '/opt/deploy/sdkwork-space/sdkwork-llm';
const topoPath = path.join(llmRoot, 'specs', 'topology.spec.json');
const topo = JSON.parse(fs.readFileSync(topoPath, 'utf8'));

const isMemoryHost = (h) => /^mem(ory|-|\.)/.test(h);
const clean = (arr) => (arr ?? []).filter((h) => !isMemoryHost(h));

const hosts = topo.cloudPublicHosts;
for (const [surface, s] of Object.entries(hosts)) {
  if (Array.isArray(s.httpHosts)) {
    const b = s.httpHosts.length;
    s.httpHosts = clean(s.httpHosts);
    if (b !== s.httpHosts.length) console.log(surface, b, '->', s.httpHosts.length);
  }
  for (const [env, cfg] of Object.entries(s.environments ?? {})) {
    if (Array.isArray(cfg.httpHosts)) {
      const b = cfg.httpHosts.length;
      cfg.httpHosts = clean(cfg.httpHosts);
      if (b !== cfg.httpHosts.length) console.log(surface, env, b, '->', cfg.httpHosts.length);
    }
  }
}
fs.writeFileSync(topoPath, JSON.stringify(topo, null, 2) + '\n');
console.log('topology updated');
