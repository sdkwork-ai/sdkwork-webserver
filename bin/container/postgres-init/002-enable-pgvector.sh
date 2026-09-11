#!/bin/sh
# sdkwork-webserver standalone compose — built-in PostgreSQL bootstrap.
# Pre-creates the pgvector extension in every canonical workspace database
# (ENVIRONMENT_SPEC §7.1) as the superuser so app roles (which lack CREATE
# EXTENSION) can run the gateway knowledgebase schema migrations. Runs after
# 001-create-workspace-schemas.sh inside docker-entrypoint-initdb.d.
set -e

create_vector_in_db() {
  db_name="$1"
  schema_name="${2:-$1}"
  psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER}" --dbname "${db_name}" <<-EOSQL
CREATE SCHEMA IF NOT EXISTS "${schema_name}";
CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA "${schema_name}";
EOSQL
}

# Install the extension into the canonical workspace schema (not just public):
# gateway/knowledgebase migrations run with search_path=<schema>[,public] and
# require the vector type to be visible from that schema.
create_vector_in_db "${WEBSERVER_POSTGRES_DEV_DB:-sdkwork_ai_dev}" "${WEBSERVER_POSTGRES_DEV_DB:-sdkwork_ai_dev}"
create_vector_in_db "${WEBSERVER_POSTGRES_TEST_DB:-sdkwork_ai_test}" "${WEBSERVER_POSTGRES_TEST_DB:-sdkwork_ai_test}"
create_vector_in_db "${WEBSERVER_POSTGRES_PROD_DB:-sdkwork_ai_prod}" "${WEBSERVER_POSTGRES_PROD_DB:-sdkwork_ai_prod}"
echo "pgvector extension ensured in workspace databases and schemas"
