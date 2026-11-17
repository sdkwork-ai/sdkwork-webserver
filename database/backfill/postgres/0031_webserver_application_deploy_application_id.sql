-- Backfill: 0031_webserver_application_deploy_application_id
--
-- NOT A MIGRATION. This file is an operator-run, mapping-driven repair script
-- for the rows that existed before `webserver_application.deploy_application_id`
-- was introduced (migration 0031). The migration deliberately leaves the column
-- nullable: the mapping from a webserver carrier row to its Deploy application
-- is a fact only the Deploy inventory owns, so no statement may synthesize it.
--
-- Why there is no automatic join:
--
--   * sdkwork-deployments owns `deploy_app` (prefix `deploy_`). The webserver
--     module does not read it in its own SQL, and a cross-module foreign key is
--     not expressible, so the link is a reference held by value.
--   * `webserver_application.slug` is the webserver-side slug, not
--     `deploy_app.slug`; the two rows were created by different planes and can
--     legitimately differ. Matching them by slug or by name would guess, and a
--     guessed link is worse than a missing one.
--
-- Procedure:
--
--   1. List the unlinked rows (query 1 below) and export the list to the Deploy
--      owner, who resolves each one against the Deploy inventory:
--          SELECT id, uuid, tenant_id, slug, name FROM deploy_app
--           WHERE tenant_id = $1 AND deleted_at IS NULL;
--   2. Fill the `mapping` CTE in query 2 with only the pairs that were
--      confirmed. Leave every unresolved row out; it stays NULL and is reported
--      by query 3.
--   3. Run query 2. It is idempotent: it touches only rows whose
--      `deploy_application_id` is still NULL, so a re-run after a partial
--      confirmation pass is safe, and it never rewrites a link.
--   4. Run query 3 and confirm that every remaining NULL is a row the Deploy
--      owner could not match yet.
--   5. Once query 3 returns zero rows, a later migration may tighten the column
--      to NOT NULL. That tightening is a separate, separately reviewed change;
--      it is not part of migration 0031.
--
-- The 1:1 rule is enforced by `uk_webserver_application_deploy_application_id`
-- (partial unique index on `deploy_application_id`). A mapping that would link
-- two carrier rows to one Deploy application fails the whole transaction with a
-- unique violation, leaving every row untouched.

-- Query 1: the legacy carrier rows that still have no Deploy application.
SELECT a.tenant_id,
       a.id,
       a.uuid,
       a.slug,
       a.name,
       a.status,
       s.uuid AS site_uuid
  FROM webserver_application a
  LEFT JOIN webserver_site s
         ON s.id = a.site_id
        AND s.tenant_id = a.tenant_id
 WHERE a.deploy_application_id IS NULL
   AND a.deleted_at IS NULL
 ORDER BY a.tenant_id, a.id;

-- Query 2: apply a confirmed mapping. The template below maps nothing; replace
-- the commented sample with the confirmed `(carrier uuid, deploy_app.id)` pairs.
BEGIN;

WITH mapping (application_uuid, deploy_application_id) AS (
    VALUES
        -- ('0f6d9b1e-3a4c-4f8e-9b2a-6c1d5e8f7a01'::text, 410000000000000001::bigint)
        (NULL::text, NULL::bigint)
)
UPDATE webserver_application a
   SET deploy_application_id = m.deploy_application_id,
       updated_at = NOW(),
       version = a.version + 1
  FROM mapping m
 WHERE m.application_uuid IS NOT NULL
   AND m.deploy_application_id IS NOT NULL
   AND a.uuid = m.application_uuid
   AND a.deploy_application_id IS NULL
   AND a.deleted_at IS NULL;

COMMIT;

-- Query 3: the rows the backfill did not cover. It must equal the set the Deploy
-- owner explicitly could not match; anything else is an unfinished mapping.
SELECT a.tenant_id, a.id, a.uuid, a.slug, a.name
  FROM webserver_application a
 WHERE a.deploy_application_id IS NULL
   AND a.deleted_at IS NULL
 ORDER BY a.tenant_id, a.id;
