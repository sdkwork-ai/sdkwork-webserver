#!/bin/sh
# sdkwork-webserver standalone compose — built-in PostgreSQL bootstrap.
# Provisions canonical workspace identities (ENVIRONMENT_SPEC §7.1).
set -e

create_identity() {
  db_name="$1"
  db_user="$2"
  db_password="$3"

  psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER}" --dbname "${POSTGRES_DB}" <<-EOSQL
DO \$\$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = '${db_user}') THEN
    EXECUTE format('CREATE ROLE %I LOGIN PASSWORD %L', '${db_user}', '${db_password}');
  ELSE
    EXECUTE format('ALTER ROLE %I WITH LOGIN PASSWORD %L', '${db_user}', '${db_password}');
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = '${db_name}') THEN
    EXECUTE format('CREATE DATABASE %I OWNER %I', '${db_name}', '${db_user}');
  END IF;
  EXECUTE format('GRANT ALL PRIVILEGES ON DATABASE %I TO %I', '${db_name}', '${db_user}');
END
\$\$;
EOSQL

  psql -v ON_ERROR_STOP=1 --username "${POSTGRES_USER}" --dbname "${db_name}" <<-EOSQL
CREATE SCHEMA IF NOT EXISTS "${db_name}";
GRANT ALL ON SCHEMA "${db_name}" TO "${db_user}";
GRANT CREATE ON SCHEMA "${db_name}" TO "${db_user}";
ALTER ROLE "${db_user}" SET search_path TO "${db_name}";
EOSQL
}

create_identity \
  "${WEBSERVER_POSTGRES_DEV_DB:-sdkwork_ai_dev}" \
  "${WEBSERVER_POSTGRES_DEV_USER:-sdkwork_ai_dev}" \
  "${WEBSERVER_POSTGRES_DEV_PASSWORD:-sdkworkdev123}"

create_identity \
  "${WEBSERVER_POSTGRES_TEST_DB:-sdkwork_ai_test}" \
  "${WEBSERVER_POSTGRES_TEST_USER:-sdkwork_ai_test}" \
  "${WEBSERVER_POSTGRES_TEST_PASSWORD:-sdkworktest123}"

create_identity \
  "${WEBSERVER_POSTGRES_PROD_DB:-sdkwork_ai_prod}" \
  "${WEBSERVER_POSTGRES_PROD_USER:-sdkwork_ai_prod}" \
  "${WEBSERVER_POSTGRES_PROD_PASSWORD:-sdkworkprod123}"
