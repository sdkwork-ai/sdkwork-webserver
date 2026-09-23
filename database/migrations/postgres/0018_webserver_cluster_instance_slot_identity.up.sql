-- sdkwork:migration
-- id: 0018_webserver_cluster_instance_slot_identity
-- engine: postgres
-- module: web
-- description: Gives a cluster instance a stable identity derived from the
--   listening slot (host + bind address + bind port) instead of the operating
--   system process id. Registration previously upserted on
--   (tenant_id, host_id, process_pid); a restarted process always carries a new
--   pid, so the conflict target never matched and every restart inserted a new
--   row with a new uuid. One edge host that restarts N times left N dead
--   instance rows, none of which could ever be reused, and the operator-facing
--   inventory could not be reconciled with the real topology. Instance identity
--   is now the deployment slot (Kubernetes StatefulSet identity / Nomad alloc
--   slot / Consul service-instance model): the same host listening on the same
--   address keeps one row and one uuid across restarts, while pid, started-at,
--   uptime and restart count are run-state observations of that slot.
--   Existing duplicates are collapsed onto the newest row before the unique
--   index is created, because the index cannot be built over a table that still
--   contains the duplicated slots.
-- reversible: true
-- rollback: down-migration drops the generated identity column and restores the pid uniqueness
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

-- ---------------------------------------------------------------------------
-- 1. Stable identity column.
--
-- A slot is `bindHost:bindPort` (the address the instance actually listens on).
-- An empty/absent bind host normalizes to `0.0.0.0` so `:8080` and
-- `0.0.0.0:8080` are the same slot rather than two rows that never collide.
-- Instances that report no bind port at all (`bindPort IS NULL`) have no slot
-- identity to key on and keep the process-pid fallback, which is the only
-- identity available for them; `pid:` is namespaced so it can never collide
-- with a real `host:port` value.
--
-- The column is GENERATED ... STORED rather than application-supplied so the
-- identity is a table invariant: no writer, including a direct SQL session, can
-- register the same slot under two representations.
-- ---------------------------------------------------------------------------
ALTER TABLE webserver_cluster_instance
    ADD COLUMN IF NOT EXISTS instance_key VARCHAR(160)
    GENERATED ALWAYS AS (
        CASE
            WHEN bind_port IS NOT NULL
                THEN COALESCE(NULLIF(BTRIM(bind_host), ''), '0.0.0.0') || ':' || bind_port::TEXT
            WHEN process_pid IS NOT NULL
                THEN 'pid:' || process_pid::TEXT
            ELSE NULL
        END
    ) STORED;

COMMENT ON COLUMN webserver_cluster_instance.instance_key IS
    'Stable instance identity: `<bindHost|0.0.0.0>:<bindPort>` for listening instances, `pid:<processPid>` as the fallback when no bind port was reported (NULL when neither is known). Survives process restarts; run-state (pid, process_started_at, uptime, restart_count) is observed against it';

-- ---------------------------------------------------------------------------
-- 2. Collapse existing duplicate slots onto the newest row.
--
-- This MUST run before the unique index: the databases that hit the defect
-- already contain N rows per slot, and CREATE UNIQUE INDEX would fail on them.
-- The newest row per slot (highest updated_at, then highest id) is kept and
-- inherits the number of rows folded into it as restart evidence - each
-- collapsed row was the same slot running as a previous process. The other rows
-- are soft-deleted so they release the slot for the unique index, and their
-- lifecycle history stays auditable.
-- ---------------------------------------------------------------------------
WITH ranked AS (
    SELECT id,
           tenant_id,
           host_id,
           instance_key,
           ROW_NUMBER() OVER (
               PARTITION BY tenant_id, host_id, instance_key
               ORDER BY updated_at DESC, id DESC
           ) AS slot_rank,
           COUNT(*) OVER (
               PARTITION BY tenant_id, host_id, instance_key
           ) - 1 AS collapsed_count
    FROM webserver_cluster_instance
    WHERE deleted_at IS NULL
      AND instance_key IS NOT NULL
)
UPDATE webserver_cluster_instance AS instance
   SET restart_count = instance.restart_count + ranked.collapsed_count,
       updated_at = NOW(),
       version = instance.version + 1
  FROM ranked
 WHERE instance.id = ranked.id
   AND ranked.slot_rank = 1
   AND ranked.collapsed_count > 0;

WITH ranked AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY tenant_id, host_id, instance_key
               ORDER BY updated_at DESC, id DESC
           ) AS slot_rank
    FROM webserver_cluster_instance
    WHERE deleted_at IS NULL
      AND instance_key IS NOT NULL
)
UPDATE webserver_cluster_instance AS instance
   SET deleted_at = NOW(),
       updated_at = NOW(),
       version = instance.version + 1
  FROM ranked
 WHERE instance.id = ranked.id
   AND ranked.slot_rank > 1;

-- ---------------------------------------------------------------------------
-- 3. The slot is the registration conflict target.
--
-- Soft-deleted rows release the slot, so a removed instance never blocks its
-- own re-registration. `instance_key IS NOT NULL` keeps rows with no identity
-- at all out of the constraint (they can only be produced by a caller that
-- reports neither a bind port nor a pid, which validate_registration rejects
-- on the service plane).
-- ---------------------------------------------------------------------------
CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_cluster_instance_slot
    ON webserver_cluster_instance (tenant_id, host_id, instance_key)
    WHERE deleted_at IS NULL AND instance_key IS NOT NULL;

-- ---------------------------------------------------------------------------
-- 4. Retire pid uniqueness.
--
-- The operating system reuses process ids, so this index could not express
-- instance identity: it rejected a legitimate re-registration whenever a new
-- process happened to reuse a pid still held by another slot's live row, while
-- failing to collide on the case it was meant to catch (the same slot restarting
-- under a new pid). Keeping it alongside the slot index would make the slot
-- update fail on pid reuse, so it is dropped rather than demoted.
-- ---------------------------------------------------------------------------
DROP INDEX IF EXISTS uk_webserver_cluster_instance_process;
