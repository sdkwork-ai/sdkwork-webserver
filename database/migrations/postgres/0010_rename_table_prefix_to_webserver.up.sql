-- sdkwork:migration
-- version: 0010
-- engine: postgres
-- module: webserver
-- description: Renames the 25 sdkwork-webserver business tables from the
--   unregistered, doubly-claimed `web_` prefix to the registered `webserver_`
--   prefix (ADR-20260917-web-framework-table-prefix.md Amendment 2,
--   DATABASE_SPEC §7). PostgreSQL's `ALTER TABLE ... RENAME TO` does not rename
--   the attached indexes or constraints, so each renamed table's primary key,
--   secondary indexes and generated *_not_null constraints are renamed
--   explicitly. The rename is driven by an exact 25-table whitelist: a
--   prefix-scoped rename would also capture `web_audit_event_id_seq` and
--   `web_security_event_id_seq`, which are owned by sdkwork-web-framework
--   tables (framework_audit_event, framework_security_event) and must not move.
-- reversible: true
-- rollback: down-migration
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

DO $$
DECLARE
  pair text[];
  pairs text[][] := ARRAY[
    ARRAY['web_app_platform_target', 'webserver_app_platform_target'],
    ARRAY['web_application', 'webserver_application'],
    ARRAY['web_audit_log', 'webserver_audit_log'],
    ARRAY['web_certificate', 'webserver_certificate'],
    ARRAY['web_certificate_identifier', 'webserver_certificate_identifier'],
    ARRAY['web_certificate_node_state', 'webserver_certificate_node_state'],
    ARRAY['web_certificate_operation', 'webserver_certificate_operation'],
    ARRAY['web_certificate_secret_bundle', 'webserver_certificate_secret_bundle'],
    ARRAY['web_certificate_version', 'webserver_certificate_version'],
    ARRAY['web_deployment', 'webserver_deployment'],
    ARRAY['web_domain', 'webserver_domain'],
    ARRAY['web_domain_verification', 'webserver_domain_verification'],
    ARRAY['web_env_variable', 'webserver_env_variable'],
    ARRAY['web_health_check', 'webserver_health_check'],
    ARRAY['web_listener_certificate_binding', 'webserver_listener_certificate_binding'],
    ARRAY['web_nginx_config', 'webserver_nginx_config'],
    ARRAY['web_resolution_cache', 'webserver_resolution_cache'],
    ARRAY['web_root_domain', 'webserver_root_domain'],
    ARRAY['web_runtime_assignment', 'webserver_runtime_assignment'],
    ARRAY['web_runtime_observation', 'webserver_runtime_observation'],
    ARRAY['web_server', 'webserver_server'],
    ARRAY['web_site', 'webserver_site'],
    ARRAY['web_site_binding', 'webserver_site_binding'],
    ARRAY['web_source_version', 'webserver_source_version'],
    ARRAY['web_tls_policy', 'webserver_tls_policy']
  ];
  target_oid oid;
  existing_rows bigint;
  renamed_count integer := 0;
  rec record;
BEGIN
  -- Pre-flight. The lifecycle applies the baseline before this migration, so a
  -- database that still carries the legacy web_* tables also gains a full set of
  -- empty webserver_* twins. Those twins are discarded below with DROP ... CASCADE,
  -- which also reclaims the foreign keys they declare against each other. Prove
  -- first that the twins are a closed set: every foreign key pointing at a
  -- webserver_* table must itself sit on a webserver_* table. Otherwise CASCADE
  -- could reach outside the discard set.
  IF EXISTS (
    SELECT 1
    FROM pg_constraint con
    JOIN pg_class rc ON rc.oid = con.confrelid
    JOIN pg_class tc ON tc.oid = con.conrelid
    WHERE con.contype = 'f'
      AND rc.relnamespace = current_schema()::regnamespace
      AND rc.relname LIKE 'webserver\_%'
      AND tc.relname NOT LIKE 'webserver\_%'
  ) THEN
    RAISE EXCEPTION
      'a foreign key outside the baseline twin set references a webserver_* table; refusing to CASCADE';
  END IF;

  FOREACH pair SLICE 1 IN ARRAY pairs LOOP
    -- Fresh installs already created webserver_* from the baseline: nothing to do.
    IF to_regclass(pair[1]) IS NULL THEN
      CONTINUE;
    END IF;

    -- The baseline uses CREATE TABLE IF NOT EXISTS, so on a database that still
    -- carries the legacy web_* tables it materialises an empty webserver_* twin.
    -- Discard that empty twin so the rename below can carry the legacy rows across.
    -- Refuse loudly if the twin is not empty rather than silently discarding data.
    IF to_regclass(pair[2]) IS NOT NULL THEN
      EXECUTE format('SELECT count(*) FROM %I', pair[2]) INTO existing_rows;
      IF existing_rows > 0 THEN
        RAISE EXCEPTION
          'both % and % exist and % holds % row(s); refusing to drop it',
          pair[1], pair[2], pair[2], existing_rows;
      END IF;
      EXECUTE format('DROP TABLE %I CASCADE', pair[2]);
    END IF;

    EXECUTE format('ALTER TABLE %I RENAME TO %I', pair[1], pair[2]);
    target_oid := to_regclass(pair[2])::oid;
    renamed_count := renamed_count + 1;

    -- Constraints: primary key, unique/check/foreign-key entries, plus the
    -- *_not_null entries PostgreSQL 17+ exposes. Renaming a constraint also renames
    -- the index that backs it, which is why the index loop below only has to pick up
    -- indexes that carry no constraint.
    FOR rec IN
      SELECT c.conname AS name
      FROM pg_constraint c
      WHERE c.conrelid = target_oid
        AND c.conname LIKE '%web\_%'
    LOOP
      EXECUTE format(
        'ALTER TABLE %I RENAME CONSTRAINT %I TO %I',
        pair[2], rec.name, replace(rec.name, 'web_', 'webserver_')
      );
    END LOOP;

    -- Standalone indexes (idx_web_*, and unique indexes created outside a constraint).
    FOR rec IN
      SELECT ic.relname AS name
      FROM pg_index x
      JOIN pg_class ic ON ic.oid = x.indexrelid
      WHERE x.indrelid = target_oid
        AND ic.relname LIKE '%web\_%'
    LOOP
      EXECUTE format(
        'ALTER INDEX %I RENAME TO %I',
        rec.name, replace(rec.name, 'web_', 'webserver_')
      );
    END LOOP;

    -- Self-check: no object attached to the renamed table may still carry the old
    -- prefix. A silent miss here would leave a table that the contract no longer
    -- describes, so fail the whole migration instead.
    FOR rec IN
      SELECT c.conname AS name
      FROM pg_constraint c
      WHERE c.conrelid = target_oid
        AND c.conname LIKE '%web\_%'
      UNION ALL
      SELECT ic.relname AS name
      FROM pg_index x
      JOIN pg_class ic ON ic.oid = x.indexrelid
      WHERE x.indrelid = target_oid
        AND ic.relname LIKE '%web\_%'
    LOOP
      RAISE EXCEPTION 'object % on % still carries the legacy web_ prefix after renaming',
        rec.name, pair[2];
    END LOOP;
  END LOOP;

  RAISE NOTICE 'renamed % web_* table(s) to webserver_*', renamed_count;
END $$;
