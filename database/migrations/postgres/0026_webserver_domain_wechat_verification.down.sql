-- sdkwork:migration
-- id: 0026_webserver_domain_wechat_verification
-- engine: postgres
-- module: web
-- description: Drops the WeChat MP domain-ownership verification file table
--   and its index.
-- reversible: true
-- rollback: up-migration recreates the table
-- transactional: true
-- lock: access-exclusive
-- lock_timeout: 5s
-- statement_timeout: 120s

DROP TABLE IF EXISTS webserver_domain_wechat_verification;
