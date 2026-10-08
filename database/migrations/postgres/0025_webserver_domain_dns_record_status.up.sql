-- sdkwork:migration
-- id: 0025_webserver_domain_dns_record_status
-- engine: postgres
-- module: web
-- description: Adds the enabled/paused state to the synced DNS resolution
--   record snapshot, mirroring the providers' own per-record pause semantics
--   (Aliyun SetDomainRecordStatus, DNSPod Record.Status). Until now a synced
--   row was pure inventory: the resolution page could show what the provider
--   holds but a paused record looked identical to a serving one, which reads
--   as a resolution that works when it does not. The column stores the state
--   the snapshot was read in; the write-through pause flips it on the provider
--   first and then in the stored row, so the page and the vendor never
--   disagree about whether a name resolves.
-- reversible: true
-- rollback: down-migration drops the record_status column
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

ALTER TABLE webserver_domain_dns_record
    ADD COLUMN IF NOT EXISTS record_status VARCHAR(8) NOT NULL DEFAULT 'ENABLED';

COMMENT ON COLUMN webserver_domain_dns_record.record_status IS
    'Provider-side resolution state at snapshot time: ENABLED or DISABLED (paused)';

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'webserver_domain_dns_record'::regclass
          AND conname = 'chk_webserver_domain_dns_record_status'
    ) THEN
        ALTER TABLE webserver_domain_dns_record
            ADD CONSTRAINT chk_webserver_domain_dns_record_status
            CHECK (record_status IN ('ENABLED', 'DISABLED'));
    END IF;
END
$$;
