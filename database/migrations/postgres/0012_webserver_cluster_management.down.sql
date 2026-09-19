-- sdkwork:migration
-- id: 0012_webserver_cluster_management
-- engine: postgres
-- module: web
-- reversible: true

DROP TABLE IF EXISTS webserver_cluster_peer_message;
DROP TABLE IF EXISTS webserver_cluster_heartbeat;
DROP TABLE IF EXISTS webserver_cluster_event;
DROP TABLE IF EXISTS webserver_cluster_instance;
DROP TABLE IF EXISTS webserver_cluster_host;
DROP TABLE IF EXISTS webserver_cluster;
