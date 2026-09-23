-- sdkwork:migration
-- id: 0018_webserver_cluster_instance_slot_identity
-- engine: postgres
-- module: web
-- reversible: true
-- rollback: down-migration drops the generated identity column and restores the pid uniqueness
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

-- ---------------------------------------------------------------------------
-- 1. Restore pid uniqueness.
--
-- Rows are de-duplicated on (tenant_id, host_id, process_pid) first: the pid
-- index cannot be rebuilt over a table that already holds two live rows for one
-- pid, which the slot identity allows (a pid is no longer unique while the slot
-- index is in force). The newest row per pid is kept; the rest are soft-deleted.
-- ---------------------------------------------------------------------------
WITH ranked AS (
    SELECT id,
           ROW_NUMBER() OVER (
               PARTITION BY tenant_id, host_id, process_pid
               ORDER BY updated_at DESC, id DESC
           ) AS pid_rank
    FROM webserver_cluster_instance
    WHERE deleted_at IS NULL
      AND process_pid IS NOT NULL
)
UPDATE webserver_cluster_instance AS instance
   SET deleted_at = NOW(),
       updated_at = NOW(),
       version = instance.version + 1
  FROM ranked
 WHERE instance.id = ranked.id
   AND ranked.pid_rank > 1;

CREATE UNIQUE INDEX IF NOT EXISTS uk_webserver_cluster_instance_process
    ON webserver_cluster_instance (tenant_id, host_id, process_pid)
    WHERE deleted_at IS NULL AND process_pid IS NOT NULL;

-- ---------------------------------------------------------------------------
-- 2. Drop the slot identity.
-- ---------------------------------------------------------------------------
DROP INDEX IF EXISTS uk_webserver_cluster_instance_slot;

ALTER TABLE webserver_cluster_instance
    DROP COLUMN IF EXISTS instance_key;
