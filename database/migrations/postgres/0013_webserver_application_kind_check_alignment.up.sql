-- sdkwork:migration
-- id: 0013_webserver_application_kind_check_alignment
-- engine: postgres
-- module: web
-- description: Aligns the webserver_application.application_kind CHECK with
--   the AppKind wire contract (8 business kinds). The constraint predated the
--   multi-kind application model and rejected every non-legacy value
--   (STATIC_WEB, SPA_WEB, ...) at insert time, so creating any modern
--   application failed against a real database. The legacy WEB/API pair is
--   kept accepted for rows written before the kind model existed.
-- reversible: true
-- rollback: down-migration restores the two-value check
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 30s
-- statement_timeout: 120s

ALTER TABLE webserver_application
    DROP CONSTRAINT IF EXISTS chk_webserver_application_kind;

ALTER TABLE webserver_application
    ADD CONSTRAINT chk_webserver_application_kind CHECK (application_kind IN (
        'WEB', 'API',
        'STATIC_WEB', 'SPA_WEB', 'API_SERVICE',
        'WECHAT_MINIPROGRAM', 'DOUYIN_MINIPROGRAM',
        'IOS_APP', 'ANDROID_APP', 'HARMONYOS_APP'
    ));
