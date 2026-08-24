#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

import { fileURLToPath, pathToFileURL } from 'node:url';

const specsTools = path.join(path.dirname(fileURLToPath(import.meta.url)), '../sdkwork-specs/tools/webserver');

const { buildWebserverDocs } = await import(pathToFileURL(path.join(specsTools, 'build-from-topology.mjs')).href);
const { validateWebserverDir } = await import(pathToFileURL(path.join(specsTools, 'validate.mjs')).href);
const { moduleUsesAdaptiveWebEdge, isEdgeProxyOnlyModule, readDeployYaml } = await import(pathToFileURL(path.join(specsTools, 'expose-mode.mjs')).href);
const { detectBrowserSurfaces, detectBrowserSurfacesForWebserver } = await import(pathToFileURL(path.join(specsTools, 'adaptive-web.mjs')).href);
const { ADAPTIVE_SNIPPET_PATHS } = await import(pathToFileURL(path.join(specsTools, 'adaptive-web-snippets.mjs')).href);
const { GATEWAY_SNIPPET_PATHS } = await import(pathToFileURL(path.join(specsTools, 'gateway-snippets.mjs')).href);
const { LIFECYCLE_ENVIRONMENTS, DEPLOYMENT_PROFILES } = await import(pathToFileURL(path.join(specsTools, 'layout-v3.mjs')).href);

const workspace = process.argv[2] ?? 'E:/sdkwork-space';

const SIDECARS = [];
for (const profile of DEPLOYMENT_PROFILES) {
  for (const env of LIFECYCLE_ENVIRONMENTS) {
    SIDECARS.push(`nginx.${profile}.${env}.conf`);
  }
}

const ADAPTIVE_SNIPPETS = Object.values(ADAPTIVE_SNIPPET_PATHS);
const FORBIDDEN_ADAPTIVE = /adaptive-web|web\.(pc|h5|static)\.conf/u;

const critical = {};
const warnings = {};
const optimizations = {};
const counts = {
  totalModulesWithDeployments: 0,
  enabledModules: 0,
  disabledModules: 0,
  proxyOnlyModules: 0,
  adaptiveModules: 0,
  modulesWithPcH5: 0,
  modulesWebNoApps: 0,
  sidecarComplete: 0,
  sidecarIncomplete: 0,
  missingReadme: 0,
  lineCountAnomalies: 0,
};

function add(bucket, category, module, detail) {
  if (!bucket[category]) bucket[category] = [];
  bucket[category].push({ module, detail });
}

function countHosts(topology) {
  const ingress = topology?.cloudPublicHosts?.['application.public-ingress'];
  if (!ingress) return 0;
  if (Array.isArray(ingress.httpHosts) && ingress.httpHosts.length) return ingress.httpHosts.length;
  if (ingress.httpHost) return 1;
  return 0;
}

function loadTopology(moduleRoot) {
  const topologyPath = path.join(moduleRoot, 'specs/topology.spec.json');
  if (fs.existsSync(topologyPath)) {
    return JSON.parse(fs.readFileSync(topologyPath, 'utf8'));
  }
  return null;
}

function getExposeModes(deployDoc) {
  const modes = new Set();
  if (!deployDoc) return modes;
  const blocks = deployDoc.profiles ? Object.values(deployDoc.profiles) : [deployDoc];
  for (const block of blocks) {
    for (const item of block?.expose ?? []) {
      if (typeof item === 'object' && item?.mode) modes.add(String(item.mode));
    }
  }
  for (const item of deployDoc.expose ?? []) {
    if (typeof item === 'object' && item?.mode) modes.add(String(item.mode));
  }
  return modes;
}

function hasWebMode(modes) {
  return modes.has('web') || modes.has('web+api');
}

function hasApiMode(modes) {
  return modes.has('api') || modes.has('web+api');
}

for (const name of fs.readdirSync(workspace).filter((n) => n.startsWith('sdkwork-')).sort()) {
  const moduleRoot = path.join(workspace, name);
  if (!fs.existsSync(path.join(moduleRoot, 'deployments'))) continue;
  counts.totalModulesWithDeployments += 1;

  const webserverDir = path.join(moduleRoot, 'deployments/webserver');
  const topology = loadTopology(moduleRoot);
  const hostCount = countHosts(topology);
  const docs = topology?.cloudPublicHosts
    ? buildWebserverDocs({ appId: name, topology, moduleRoot })
    : { enabled: false };
  const enabled = docs.enabled && hostCount > 0;

  if (!enabled) {
    counts.disabledModules += 1;
    continue;
  }
  counts.enabledModules += 1;

  const deployDoc = readDeployYaml(moduleRoot);
  const exposeModes = getExposeModes(deployDoc);
  const proxyOnly = isEdgeProxyOnlyModule(name);
  const adaptive = moduleUsesAdaptiveWebEdge(moduleRoot, name);
  if (proxyOnly) counts.proxyOnlyModules += 1;
  if (adaptive) counts.adaptiveModules += 1;

  const browser = detectBrowserSurfacesForWebserver(moduleRoot, webserverDir);
  if (browser.pcExists || browser.h5Exists) counts.modulesWithPcH5 += 1;
  if (hasWebMode(exposeModes) && !browser.pcExists && !browser.h5Exists) {
    counts.modulesWebNoApps += 1;
    add(
      warnings,
      'web_mode_no_pc_h5_apps',
      name,
      'expose.mode includes web/web+api but no apps/*-pc or apps/*-h5 (collapse/static-fallback expected)',
    );
  }

  const missingSidecars = SIDECARS.filter((file) => !fs.existsSync(path.join(webserverDir, file)));
  if (missingSidecars.length > 0) {
    counts.sidecarIncomplete += 1;
    add(
      critical,
      'missing_nginx_sidecars',
      name,
      `missing ${missingSidecars.length}/8: ${missingSidecars.join(', ')}`,
    );
  } else {
    counts.sidecarComplete += 1;
  }

  const snippetsDir = path.join(webserverDir, 'snippets');
  if (proxyOnly) {
    if (fs.existsSync(snippetsDir)) {
      for (const entry of fs.readdirSync(snippetsDir)) {
        if (FORBIDDEN_ADAPTIVE.test(entry)) {
          add(
            critical,
            'forbidden_adaptive_snippets_proxy_only',
            name,
            `snippets/${entry} forbidden on proxy-only module`,
          );
        }
      }
    }
    for (const snippet of [
      GATEWAY_SNIPPET_PATHS.production,
      GATEWAY_SNIPPET_PATHS.nonproduction,
    ]) {
      if (!fs.existsSync(path.join(webserverDir, snippet))) {
        add(critical, 'missing_gateway_snippets', name, `missing ${snippet}`);
      }
    }
  } else if (adaptive) {
    for (const snippet of [
      ...ADAPTIVE_SNIPPETS,
      GATEWAY_SNIPPET_PATHS.apiProduction,
      GATEWAY_SNIPPET_PATHS.nonproduction,
    ]) {
      if (!fs.existsSync(path.join(webserverDir, snippet))) {
        add(critical, 'missing_adaptive_snippets', name, `missing ${snippet}`);
      }
    }
  } else if (hasApiMode(exposeModes)) {
    for (const snippet of [
      GATEWAY_SNIPPET_PATHS.production,
      GATEWAY_SNIPPET_PATHS.nonproduction,
    ]) {
      if (!fs.existsSync(path.join(webserverDir, snippet))) {
        add(warnings, 'missing_api_snippets', name, `api-mode module missing ${snippet}`);
      }
    }
  }

  const rawBrowser = detectBrowserSurfaces(moduleRoot);
  const hasApps = rawBrowser.pcExists || rawBrowser.h5Exists;
  const appRootsPath = path.join(webserverDir, 'app-roots.example.toml');
  if (hasApps && !fs.existsSync(appRootsPath)) {
    add(
      warnings,
      'missing_app_roots_example',
      name,
      'apps/*-pc or apps/*-h5 exist but app-roots.example.toml missing',
    );
  }

  const staticDir = path.join(webserverDir, 'static');
  if (adaptive && !fs.existsSync(staticDir)) {
    add(
      warnings,
      'missing_static_dir',
      name,
      'adaptive module missing deployments/webserver/static/',
    );
  }

  const validation = validateWebserverDir(moduleRoot);
  for (const warning of validation.warnings ?? []) {
    if (warning.includes('(W18)')) add(warnings, 'deploy_yaml_w18_mismatch', name, warning);
  }
  for (const error of validation.errors ?? []) {
    if (error.includes('(W29)')) add(critical, 'w29_adaptive_wiring', name, error);
    else if (error.includes('(W23)')) add(critical, 'w23_proxy_only_violation', name, error);
    else if (error.includes('(W16)')) add(critical, 'w16_sidecar_drift', name, error);
    else if (error.includes('(W26)')) add(critical, 'w26_env_parity', name, error);
    else if (error.includes('(W28)')) add(critical, 'w28_missing_snippet_ref', name, error);
    else if (!validation.ok) add(critical, 'validation_errors', name, error);
  }

  if (!fs.existsSync(path.join(webserverDir, 'README.md'))) {
    counts.missingReadme += 1;
    add(warnings, 'missing_readme', name, 'deployments/webserver/README.md missing');
  }

  const sidecarLines = {};
  for (const file of SIDECARS) {
    const filePath = path.join(webserverDir, file);
    if (fs.existsSync(filePath)) {
      sidecarLines[file] = fs.readFileSync(filePath, 'utf8').split('\n').length;
    }
  }
  if (Object.keys(sidecarLines).length === 8) {
    const values = Object.values(sidecarLines);
    const min = Math.min(...values);
    const max = Math.max(...values);
    if (max > min * 3 && max - min > 50) {
      counts.lineCountAnomalies += 1;
      add(
        optimizations,
        'sidecar_line_count_anomaly',
        name,
        `sidecar lines range ${min}-${max}`,
      );
    }
  }

  for (const environment of LIFECYCLE_ENVIRONMENTS) {
    const standalonePath = path.join(webserverDir, `nginx.standalone.${environment}.conf`);
    const cloudPath = path.join(webserverDir, `nginx.cloud.${environment}.conf`);
    if (fs.existsSync(standalonePath) && fs.existsSync(cloudPath)) {
      const standaloneText = fs.readFileSync(standalonePath, 'utf8').replace(/#.*$/gm, '').trim();
      const cloudText = fs.readFileSync(cloudPath, 'utf8').replace(/#.*$/gm, '').trim();
      if (standaloneText === cloudText) {
        add(
          optimizations,
          'identical_standalone_cloud_sidecars',
          name,
          `${environment}: standalone and cloud sidecars are identical`,
        );
      }
    }
  }

  const retired = path.join(webserverDir, 'server.toml');
  if (fs.existsSync(retired)) {
    add(critical, 'retired_server_toml', name, 'server.toml still present (layout v3 migration incomplete)');
  }
}

function summarize(bucket) {
  const summary = {};
  for (const [category, items] of Object.entries(bucket)) {
    summary[category] = {
      count: items.length,
      modules: [...new Set(items.map((item) => item.module))].sort(),
      details: items,
    };
  }
  return summary;
}

console.log(JSON.stringify({
  counts,
  critical: summarize(critical),
  warnings: summarize(warnings),
  optimizations: summarize(optimizations),
}, null, 2));
