-- sdkwork:migration
-- id: 0019_webserver_certificate_operation_failure_detail
-- engine: postgres
-- module: web
-- description: Records what a failed certificate operation actually said, next
--   to the stable code that classifies it. The operation row carried only
--   `failure_code` (`ACME_PROVIDER_FAILED`, `CERTIFICATE_OPERATION_INVALID`, …),
--   which is deliberately machine-readable and therefore deliberately
--   information-free: a DNS provider refusing a credential, a zone the
--   credential cannot see and a revoked token all produced the same string, and
--   the provider's own diagnostic ("Authentication error (10000)",
--   "InvalidAccessKeyId.NotFound") existed only inside a `tracing::warn!` line
--   on the server. An operator reading the console could see that issuance
--   failed and could not see why, so the only way to act was to find the host
--   and read its log. The detail is a separate column rather than a
--   concatenation into `failure_code` because the code is what the console
--   keys its localized copy off, and it is what retry and cooldown logic
--   branches on; widening it into prose would break both. The detail is
--   redacted and bounded before it is stored: it is provider text, and the
--   HTTP_REQUEST provider family carries its credentials inside
--   operator-authored templates that a provider may quote back.
-- reversible: true
-- rollback: down-migration drops the failure_detail column and its constraint
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

-- ---------------------------------------------------------------------------
-- 1. The diagnostic column.
--
-- VARCHAR(512) is a character count in PostgreSQL, so a Chinese provider's
-- message fits without being counted as three bytes per character. The ceiling
-- matches the service plane's own bound on the text it will store: the
-- provider body excerpt is capped before it ever reaches SQL.
-- ---------------------------------------------------------------------------
ALTER TABLE webserver_certificate_operation
    ADD COLUMN IF NOT EXISTS failure_detail VARCHAR(512);

COMMENT ON COLUMN webserver_certificate_operation.failure_detail IS
    'What the failure actually said: the provider''s own diagnostic, redacted and bounded, so an operator can act on it without reading the server log';

-- ---------------------------------------------------------------------------
-- 2. A detail only ever accompanies a code.
--
-- The baseline declares this constraint inline for fresh installations, and an
-- already-installed database needs it added. PostgreSQL has no
-- `ADD CONSTRAINT IF NOT EXISTS`, so the guard is the catalog itself, which also
-- makes this re-runnable per the expand-only convention.
--
-- The invariant is worth enforcing in the table rather than trusting writers:
-- every path that clears `failure_code` (a re-claim, a re-issue, an expiry
-- sweep) has to clear the detail with it, and a detail left behind on a row
-- whose code is gone would render in the console as a failure that no longer
-- exists.
-- ---------------------------------------------------------------------------
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'webserver_certificate_operation'::regclass
          AND conname = 'chk_webserver_certificate_operation_failure_detail'
    ) THEN
        ALTER TABLE webserver_certificate_operation
            ADD CONSTRAINT chk_webserver_certificate_operation_failure_detail CHECK (
                failure_detail IS NULL OR failure_code IS NOT NULL
            );
    END IF;
END
$$;
