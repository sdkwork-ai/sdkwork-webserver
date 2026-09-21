-- sdkwork:migration
-- id: 0015_webserver_cluster_load_balancing
-- engine: postgres
-- module: web
-- reversible: true

ALTER TABLE webserver_cluster
    DROP COLUMN IF EXISTS served_domains,
    DROP COLUMN IF EXISTS lb_strategy;
