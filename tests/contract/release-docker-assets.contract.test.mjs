import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { parse as parseYaml } from 'yaml';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const COMPOSE = path.join(REPO_ROOT, 'deployments', 'docker', 'docker-compose.release.yml');
const ENTRYPOINT = path.join(
  REPO_ROOT, 'deployments', 'docker', 'docker', 'entrypoint-release.sh',
);

// Windows checkouts are CRLF while the shipped file is LF; `$`-anchored
// patterns would otherwise match in CI and not here.
function readNormalized(...segments) {
  return readFileSync(path.join(...segments), 'utf8').replace(/\r\n/gu, '\n');
}

function readCompose() {
  return parseYaml(readNormalized(COMPOSE));
}

// Every environment key a service declares, flattened across the two shapes
// compose accepts (`KEY: value` and the `- KEY=value` list form).
function environmentKeys(service) {
  const environment = service?.environment;
  if (!environment) return [];
  if (Array.isArray(environment)) {
    return environment.map((entry) => String(entry).split('=', 1)[0]);
  }
  return Object.keys(environment);
}

test('the bundled database role and the application share one password file', () => {
  const compose = readCompose();
  const secrets = compose.secrets ?? {};
  assert.ok(
    secrets.database_password,
    'compose must declare a database_password secret so the role password has one source',
  );
  assert.match(
    String(secrets.database_password.file),
    /secrets\/database\.secret$/u,
    'the database secret must be sourced from the same database.secret config.toml reads',
  );

  const db = compose.services?.db;
  assert.equal(
    db?.environment?.POSTGRES_PASSWORD_FILE,
    '/run/secrets/database_password',
    'postgres must read the password from the mounted file',
  );
  assert.ok(
    !('POSTGRES_PASSWORD' in (db?.environment ?? {})),
    'an inline POSTGRES_PASSWORD would be a second, silently divergent credential',
  );
  assert.ok(
    (db?.secrets ?? []).includes('database_password'),
    'the db service must mount the database_password secret',
  );

  const webserverEnvironment = environmentKeys(compose.services?.webserver);
  for (const key of ['SDKWORK_DATABASE_HOST', 'SDKWORK_DATABASE_NAME', 'SDKWORK_DATABASE_USERNAME']) {
    assert.ok(
      !webserverEnvironment.includes(key),
      `${key} is materialised from config.toml at startup, so injecting it here is a no-op knob`,
    );
  }
});

test('the bundled redis password stays out of .env and reaches the cache and the app', () => {
  const compose = readCompose();
  const redisSecret = compose.secrets?.redis_password;
  assert.ok(redisSecret, 'compose must declare a redis_password secret');
  assert.match(
    String(redisSecret.file),
    /secrets\/redis\.secret$/u,
    'the redis secret must be file-sourced, not an inline .env value',
  );

  const cache = compose.services?.cache;
  assert.ok(
    (cache?.secrets ?? []).includes('redis_password'),
    'the cache service must mount the redis_password secret',
  );
  // redis-server has no password-file flag, so the value has to reach argv via a
  // shell; `$$` must survive so Compose does not interpolate the inner shell.
  const cacheArgv = JSON.stringify(cache?.entrypoint ?? cache?.command ?? []);
  assert.match(cacheArgv, /run\/secrets\/redis_password/u, 'the cache must read its password from the secret');
  assert.match(cacheArgv, /requirepass/u);
  assert.ok(!cacheArgv.includes('${REDIS_PASSWORD'), 'the cache password must not come from .env');

  const healthcheck = JSON.stringify(cache?.healthcheck?.test ?? []);
  assert.match(healthcheck, /redis_password/u, 'the healthcheck must authenticate with the same secret file');

  const entrypoint = readNormalized(ENTRYPOINT);
  assert.match(
    entrypoint,
    /REDIS_SECRET_FILE="\$\{CONFIG_ROOT\}\/secrets\/redis\.secret"/u,
    'the entrypoint must read the redis password from the mounted secret file',
  );
  assert.match(
    entrypoint,
    /SDKWORK_WEBSERVER_REDIS_URL="redis:\/\/:\$\{REDIS_PASSWORD\}@\$\{REDIS_HOST\}:\$\{REDIS_PORT\}"/u,
    'the entrypoint must build the resolver-cache url from that file',
  );
  // The cache layer is optional and connects lazily: a missing or empty secret
  // must degrade to the layers above, never abort startup.
  const guardIndex = entrypoint.indexOf('if [ -z "${SDKWORK_WEBSERVER_REDIS_URL:-}" ]');
  const execIndex = entrypoint.indexOf('exec "${GATEWAY_BINARY}"');
  assert.ok(guardIndex !== -1 && execIndex > guardIndex, 'the redis block must run before the gateway exec');
  const redisBlock = entrypoint.slice(guardIndex, execIndex);
  assert.match(
    redisBlock,
    /&& \[ -r "\$\{REDIS_SECRET_FILE\}" \]/u,
    'the redis url must only be built from a secret file that is actually readable',
  );
  assert.ok(
    !/\b(?:fail|exit)\b/u.test(redisBlock),
    'a missing or empty redis secret must degrade, not abort startup',
  );
});

test('compose variable interpolation stays confined to operator-facing knobs', () => {
  const source = readNormalized(COMPOSE);
  // `${...}` in a compose file is substituted by Compose itself; a stray one
  // inside a command becomes an empty string at runtime and fails silently.
  const innerShellReferences = [...source.matchAll(/\$\$\(/gu)];
  assert.ok(innerShellReferences.length > 0, 'inner-shell command substitutions must be escaped as $$(...)');

  for (const match of source.matchAll(/\$\{([A-Z0-9_]+)([:?-][^}]*)?\}/gu)) {
    const name = match[1];
    assert.ok(
      /^(SDKWORK_VERSION|HOST_HTTP_PORT|CONTAINER_HTTP_PORT|POSTGRES_DB|POSTGRES_USER|SDKWORK_CONFIG_DIR)$/u.test(name),
      `${name} is interpolated but is not a declared operator knob`,
    );
  }
});
