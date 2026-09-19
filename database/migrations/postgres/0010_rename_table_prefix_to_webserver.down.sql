-- sdkwork:migration
-- version: 0010
-- engine: postgres
-- module: webserver
-- description: Rollback of 0010_rename_table_prefix_to_webserver: renames the 25
--   sdkwork-webserver business tables from `webserver_` back to the legacy `web_`
--   prefix, together with their primary keys, indexes, constraints and generated
--   *_not_null entries. The old prefix is unregistered and doubly claimed, so this
--   down migration exists only to restore a pre-Amendment-2 development database.
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
    ARRAY['webserver_app_platform_target', 'web_app_platform_target'],
    ARRAY['webserver_application', 'web_application'],
    ARRAY['webserver_audit_log', 'web_audit_log'],
    ARRAY['webserver_certificate', 'web_certificate'],
    ARRAY['webserver_certificate_identifier', 'web_certificate_identifier'],
    ARRAY['webserver_certificate_node_state', 'web_certificate_node_state'],
    ARRAY['webserver_certificate_operation', 'web_certificate_operation'],
    ARRAY['webserver_certificate_secret_bundle', 'web_certificate_secret_bundle'],
    ARRAY['webserver_certificate_version', 'web_certificate_version'],
    ARRAY['webserver_deployment', 'web_deployment'],
    ARRAY['webserver_domain', 'web_domain'],
    ARRAY['webserver_domain_verification', 'web_domain_verification'],
    ARRAY['webserver_env_variable', 'web_env_variable'],
    ARRAY['webserver_health_check', 'web_health_check'],
    ARRAY['webserver_listener_certificate_binding', 'web_listener_certificate_binding'],
    ARRAY['webserver_nginx_config', 'web_nginx_config'],
    ARRAY['webserver_resolution_cache', 'web_resolution_cache'],
    ARRAY['webserver_root_domain', 'web_root_domain'],
    ARRAY['webserver_runtime_assignment', 'web_runtime_assignment'],
    ARRAY['webserver_runtime_observation', 'web_runtime_observation'],
    ARRAY['webserver_server', 'web_server'],
    ARRAY['webserver_site', 'web_site'],
    ARRAY['webserver_site_binding', 'web_site_binding'],
    ARRAY['webserver_source_version', 'web_source_version'],
    ARRAY['webserver_tls_policy', 'web_tls_policy']
  ];
  target_oid oid;
  existing_rows bigint;
  renamed_count integer := 0;
  rec record;
BEGIN
  -- Pre-flight, mirroring the up migration: DROP ... CASCADE is only safe while
  -- the discarded twins form a closed set of foreign-key dependents.
  IF EXISTS (
    SELECT 1
    FROM pg_constraint con
    JOIN pg_class rc ON rc.oid = con.confrelid
    JOIN pg_class tc ON tc.oid = con.conrelid
    WHERE con.contype = 'f'
      AND rc.relnamespace = current_schema()::regnamespace
      AND rc.relname LIKE 'web\_%'
      AND tc.relname NOT LIKE 'web\_%'
  ) THEN
    RAISE EXCEPTION
      'a foreign key outside the legacy twin set references a web_* table; refusing to CASCADE';
  END IF;

  FOREACH pair SLICE 1 IN ARRAY pairs LOOP
    IF to_regclass(pair[1]) IS NULL THEN
      CONTINUE;
    END IF;

    -- A pre-existing legacy table with rows would collide with the rename. Refuse
    -- rather than drop anything that might hold data.
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

    FOR rec IN
      SELECT c.conname AS name
      FROM pg_constraint c
      WHERE c.conrelid = target_oid
        AND c.conname LIKE '%webserver\_%'
    LOOP
      EXECUTE format(
        'ALTER TABLE %I RENAME CONSTRAINT %I TO %I',
        pair[2], rec.name, replace(rec.name, 'webserver_', 'web_')
      );
    END LOOP;

    FOR rec IN
      SELECT ic.relname AS name
      FROM pg_index x
      JOIN pg_class ic ON ic.oid = x.indexrelid
      WHERE x.indrelid = target_oid
        AND ic.relname LIKE '%webserver\_%'
    LOOP
      EXECUTE format(
        'ALTER INDEX %I RENAME TO %I',
        rec.name, replace(rec.name, 'webserver_', 'web_')
      );
    END LOOP;

    FOR rec IN
      SELECT c.conname AS name
      FROM pg_constraint c
      WHERE c.conrelid = target_oid
        AND c.conname LIKE '%webserver\_%'
      UNION ALL
      SELECT ic.relname AS name
      FROM pg_index x
      JOIN pg_class ic ON ic.oid = x.indexrelid
      WHERE x.indrelid = target_oid
        AND ic.relname LIKE '%webserver\_%'
    LOOP
      RAISE EXCEPTION 'object % on % still carries the webserver_ prefix after renaming',
        rec.name, pair[2];
    END LOOP;
  END LOOP;

  RAISE NOTICE 'restored % webserver_* table(s) to web_*', renamed_count;
END $$;
