-- sdkwork-webserver external-dependency mode (docker-compose.external.yml)
-- Run against an EXISTING PostgreSQL instance before starting the gateway in
-- external mode. The gateway connects with SDKWORK_DATABASE_* to the canonical
-- database; the sdkwork-database lifecycle pins search_path to the same-named
-- schema.
--
-- Example (as postgres superuser):
--   psql -h <host> -U postgres -c "CREATE DATABASE sdkwork_ai_dev OWNER sdkwork_ai_dev;"
--   psql -h <host> -U postgres -d sdkwork_ai_dev -v db=sdkwork_ai_dev \
--     -v app_user=sdkwork_ai_dev -f deployments/docker/postgres/external-schema.sql
--
-- Optional psql variables:
--   db         database and schema name (default sdkwork_ai_dev)
--   app_user   gateway role (default: same as db)

\if :{?db}
\else
\set db sdkwork_ai_dev
\endif
\if :{?app_user}
\else
\set app_user :db
\endif

SELECT format('CREATE SCHEMA IF NOT EXISTS %I', :'db') \gexec
SELECT format('GRANT ALL ON SCHEMA %I TO %I', :'db', :'app_user') \gexec
SELECT format('GRANT CREATE ON SCHEMA %I TO %I', :'db', :'app_user') \gexec
